// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/PrepareInlineable.scala
//   compiler/src/dotty/tools/dotc/transform/AccessProxies.scala
//! The definitions of inline accessors that take the accessed member's receiver as their first
//! parameter (dotty's `MakeInlineablePassing`, `accessorDef` with `passReceiverAsArg`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

/// The name the receiver parameter of a setter has until it is typed, renamed to `x$0` then.
pub const SETTER_RECEIVER: &str = "x$0$receiver";

impl Typer<'_> {
    /// Replaces selection `t` of `sym`, which no class around `ctx.owner` can host an accessor
    /// of, by a call of an accessor in the class of inline method `meth` that takes the
    /// selection's qualifier, or assignment `assign` to it by a call of such a setter; whether
    /// it did (dotty's `MakeInlineablePassing`, `useSetter`).
    pub fn pass_receiver(
        &mut self,
        t: TNodeId,
        sym: SymbolId,
        meth: SymbolId,
        assign: Option<TNodeId>,
        ctx: Ctx,
    ) -> bool {
        let T::Select { qualifier, .. } = *self.tpd.tree(t) else { return false };
        if self.u.is_constructor(sym) || self.accessor_host(sym, ctx.owner).is_some() {
            return false;
        }
        let Some(host) = self.u.symbol(meth).owner.filter(|&c| self.u.symbol(c).kind == Kind::Class) else {
            return false;
        };
        let span = self.tpd.span(t);
        let widened = self.u.widen(self.tpd.tpe(qualifier));
        let receiver = self.without_local_types(widened, meth);
        if receiver != widened {
            self.complete(sym);
            let seen = self.u.member_info(widened, sym);
            if self.mentions_local_type(seen, meth) {
                self.unsupported_at(
                    span,
                    "inline accessor of a member whose type has the inline method's types",
                );
                return false;
            }
        }
        let name = self.u.symbol(sym).name.clone();
        let unique = self.fresh.next(self.names, &format!("{name}$i"));
        let suffix = self.names.text(unique)[name.len()..].to_owned();
        let getter = format!("inline${name}{suffix}");
        let Some(getter) = self.passing_accessor(host, &getter, sym, receiver, false, span, ctx) else {
            return false;
        };
        let target = match assign {
            None => getter,
            Some(_) => {
                let setter = format!("inline${name}_={suffix}");
                let Some(s) = self.passing_accessor(host, &setter, sym, receiver, true, span, ctx) else {
                    return false;
                };
                s
            }
        };
        self.complete(target);
        let info = self.u.info(target);
        let Type::Method { result, .. } = *self.u.types().get(info) else { return false };
        let fun = self.member_ref(target, Span::new(span.start(), span.end(), span.start()));
        let args = self.tpd.list(&[qualifier]);
        match assign {
            None => {
                let node = self.tpd.get_mut(t);
                (node.tree, node.tpe, node.sym) = (T::Apply { fun, args }, Some(result), None);
            }
            Some(a) => {
                let T::Assign { rhs, .. } = *self.tpd.tree(a) else { return false };
                let fun = self.tpd.node(T::Apply { fun, args }, span, Some(result), None);
                let unit = self.tpd.tpe(a);
                let args = self.tpd.list(&[rhs]);
                let node = self.tpd.get_mut(a);
                (node.tree, node.tpe) = (T::Apply { fun, args }, Some(unit));
            }
        }
        true
    }

    /// Type `t` with its type arguments that refer to types defined in method `meth` replaced
    /// by wildcards, which erase alike (in place of dotty's type parameters of the accessor).
    fn without_local_types(&mut self, t: TypeId, meth: SymbolId) -> TypeId {
        self.u.map_type(t, &mut |u, part| {
            let Type::Applied(tycon, args) = u.types().get(part).clone() else { return None };
            let args = args.into_iter().map(|a| match mentions_local(u, a, meth) {
                true => u.intern_type(Type::Bounds(tasty::NOTHING, tasty::ANY)),
                false => a,
            });
            let args = args.collect();
            Some(u.intern_type(Type::Applied(tycon, args)))
        })
    }

    fn mentions_local_type(&mut self, t: TypeId, meth: SymbolId) -> bool {
        mentions_local(self.u, t, meth)
    }

    /// `def name(x$0: R)(ps): T = x$0.accessed(ps)` for receivers of type `receiver`, or
    /// `def name(x$0: R)(x$0: T): Unit = x$0.accessed = x$0` for a setter, and the receiver's
    /// parameter; `None`, reported at `span`, for a polymorphic or implicit method.
    pub fn passing_def(
        &mut self,
        name: &str,
        accessed: SymbolId,
        receiver: TypeId,
        setter: bool,
        span: Span,
    ) -> Option<(NodeId, NodeId)> {
        self.complete(accessed);
        let mut info = self.u.member_info(receiver, accessed);
        let mut clauses: Vec<Vec<(String, TypeId)>> = Vec::new();
        loop {
            match self.u.types().get(info).clone() {
                Type::Method { params, result, implicit: false, .. } if !setter => {
                    clauses.push(params);
                    info = result;
                }
                Type::Method { .. } | Type::Poly { .. } => {
                    self.unsupported_at(span, "inline accessor of a polymorphic or implicit method");
                    return None;
                }
                _ => break,
            }
        }
        if let Type::ByName(r) = *self.u.types().get(info) {
            info = r;
        }
        let at = self.syms.def_trees.get(&accessed).map_or(span, |&d| self.ast.span(d));
        let at = Span::at(at.point());
        let own = if setter { SETTER_RECEIVER } else { "x$0" };
        let recv_tpt = self.splice_type(receiver);
        let recv = self.passing_param(own, recv_tpt);
        let mut defs = vec![self.ast.list(&[recv])];
        let own_name = TName { name: self.name(own), is_type: false };
        let qualifier = self.ast.node(Tree::Ident { name: own_name });
        let member = TName { name: self.name(&self.u.symbol(accessed).name.clone()), is_type: false };
        let mut rhs = self.ast.node(Tree::Select { qualifier, name: member });
        let result = if setter {
            clauses.push(vec![("x$0".to_owned(), info)]);
            crate::defn::type_ref(self.u, self.defn.unit)
        } else {
            info
        };
        for clause in &clauses {
            let mut params = Vec::new();
            let mut args = Vec::new();
            for (pname, ptype) in clause {
                let tpt = self.splice_type(*ptype);
                params.push(self.passing_param(pname, tpt));
                let name = TName { name: self.name(pname), is_type: false };
                args.push(self.ast.node(Tree::Ident { name }));
            }
            rhs = match setter {
                true => self.ast.node(Tree::Assign { lhs: rhs, rhs: args[0] }),
                false => {
                    let args = self.ast.list(&args);
                    self.ast.node(Tree::Apply { fun: rhs, args })
                }
            };
            defs.push(self.ast.list(&params));
        }
        let tpt = self.splice_type(result);
        let paramss = self.ast.clauses(&defs);
        let private = self.u.symbol(accessed).flags.has(tasty::tags::PRIVATE);
        let flags = f::SYNTHETIC.or(if private { f::FINAL } else { f::EMPTY });
        let mods = self.ast.mods(Modifiers::with_flags(flags));
        let name = TName { name: self.name(name), is_type: false };
        let def = self.ast.node(Tree::DefDef { name, paramss, tpt, rhs: Some(rhs), mods });
        self.ast.with_span(def, at);
        Some((def, recv))
    }

    fn passing_param(&mut self, name: &str, tpt: NodeId) -> NodeId {
        let name = TName { name: self.name(name), is_type: false };
        let mods = self.ast.mods(Modifiers::with_flags(f::TERM_PARAM));
        self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods })
    }
}

/// Whether type `t` refers to a type defined in method `meth`.
fn mentions_local(u: &mut tasty::Universe, t: TypeId, meth: SymbolId) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, p| {
        if let Type::Sym(_, s) = *u.types().get(p) {
            found |= crate::sealed::is_contained_in(u, s, meth);
        }
        None
    });
    found
}
