// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/inlines/PrepareInlineable.scala
//   compiler/src/dotty/tools/dotc/transform/AccessProxies.scala
//! The definitions of inline accessors that take the accessed member's receiver as their first
//! parameter (dotty's `MakeInlineablePassing`, `accessorDef` with `passReceiverAsArg`).

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{SymbolId, Type, TypeId, Universe};

/// The name the receiver parameter of a setter has until it is typed, renamed to `x$0` then.
pub const SETTER_RECEIVER: &str = "x$0$receiver";

impl Typer<'_> {
    /// `def name[Ls](x$0: R)(ps): T = x$0.accessed(ps)` for receivers of type `R` of
    /// `receiver`, `Ls` its types defined in the inline method, or
    /// `def name[Ls](x$0: R)(x$0: T): Unit = x$0.accessed = x$0` for a setter; `None`, reported
    /// at `span`, for a polymorphic or implicit method.
    pub fn passing_def(
        &mut self,
        name: &str,
        accessed: SymbolId,
        (receiver, locals): (TypeId, &[SymbolId]),
        setter: bool,
        span: Span,
    ) -> Option<NodeId> {
        self.complete(accessed);
        let info = self.u.member_info(receiver, accessed);
        let mut info = dealiased(self.u, info);
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
        let mut defs = Vec::new();
        if !locals.is_empty() {
            let mut tparams = Vec::new();
            for &l in locals {
                let name = TName { name: self.name(&self.u.symbol(l).name.clone()), is_type: true };
                let bounds = self.u.info(l);
                let rhs = self.passing_tpt(bounds, locals, span)?;
                let mods = self.ast.mods(Modifiers::with_flags(f::TYPE_PARAM));
                tparams.push(self.ast.node(Tree::TypeDef { name, rhs, mods }));
            }
            defs.push(self.ast.list(&tparams));
        }
        let own = if setter { SETTER_RECEIVER } else { "x$0" };
        let recv_tpt = self.passing_tpt(receiver, locals, span)?;
        let recv = self.passing_param(own, recv_tpt);
        defs.push(self.ast.list(&[recv]));
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
                let tpt = self.passing_tpt(*ptype, locals, span)?;
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
        let tpt = self.passing_tpt(result, locals, span)?;
        let paramss = self.ast.clauses(&defs);
        let private = self.u.symbol(accessed).flags.has(tasty::tags::PRIVATE);
        let flags = f::SYNTHETIC.or(if private { f::FINAL } else { f::EMPTY });
        let mods = self.ast.mods(Modifiers::with_flags(flags));
        let name = TName { name: self.name(name), is_type: false };
        let def = self.ast.node(Tree::DefDef { name, paramss, tpt, rhs: Some(rhs), mods });
        self.ast.with_span(def, at);
        Some(def)
    }

    /// Names the receiver of typed passing setter definition `def`, and its reference, `x$0`.
    pub fn with_setter_receiver(&mut self, def: TNodeId) {
        let T::DefDef { paramss, rhs: Some(rhs), .. } = *self.tpd.tree(def) else { return };
        let clauses = self.tpd.clause_items(paramss);
        let params = clauses.iter().flat_map(|&c| self.tpd.items(c));
        let Some(&recv) = params.clone().find(|&&p| matches!(self.tpd.tree(p), T::ValDef { .. })) else {
            return;
        };
        let T::ValDef { name, tpt, rhs: none } = *self.tpd.tree(recv) else { return };
        if self.names.text(name.name) != SETTER_RECEIVER {
            return;
        }
        let name = TName { name: self.name("x$0"), is_type: false };
        self.tpd.get_mut(recv).tree = T::ValDef { name, tpt, rhs: none };
        let sym = self.tpd.sym(recv).expect("a parameter");
        self.u.rename(sym, "x$0");
        if let T::Assign { lhs, .. } = *self.tpd.tree(rhs)
            && let T::Select { qualifier, .. } = *self.tpd.tree(lhs)
        {
            self.tpd.get_mut(qualifier).tree = T::Ident { name };
        }
    }

    fn passing_param(&mut self, name: &str, tpt: NodeId) -> NodeId {
        let name = TName { name: self.name(name), is_type: false };
        let mods = self.ast.mods(Modifiers::with_flags(f::TERM_PARAM));
        self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods })
    }

    /// The type tree of `t` in an accessor with type parameters named as `locals`: each of them
    /// by name, applied types of them part by part, else `t` spliced; `None`, reported at
    /// `span`, for another type referring to them.
    fn passing_tpt(&mut self, t: TypeId, locals: &[SymbolId], span: Span) -> Option<NodeId> {
        if !refers_to(self.u, t, locals) {
            return Some(self.splice_type(t));
        }
        match self.u.types().get(t).clone() {
            Type::Sym(_, s) if locals.contains(&s) => {
                let name = TName { name: self.name(&self.u.symbol(s).name.clone()), is_type: true };
                Some(self.ast.node(Tree::Ident { name }))
            }
            Type::Applied(tycon, args) => {
                let tpt = self.passing_tpt(tycon, locals, span)?;
                let args = (args.into_iter().map(|a| self.passing_tpt(a, locals, span)))
                    .collect::<Option<Vec<_>>>()?;
                let args = self.ast.list(&args);
                Some(self.ast.node(Tree::AppliedTypeTree { tpt, args }))
            }
            Type::Bounds(lo, hi) => {
                let lo = Some(self.passing_tpt(lo, locals, span)?);
                let hi = Some(self.passing_tpt(hi, locals, span)?);
                Some(self.ast.node(Tree::TypeBoundsTree { lo, hi, alias: None }))
            }
            _ => {
                self.unsupported_at(span, "inline accessor of a type built of the inline method's types");
                None
            }
        }
    }
}

/// The types defined in method `meth` that type `t` refers to, in the reverse order of their
/// first occurrence (dotty's `namedPartsWith`).
pub fn local_refs(u: &mut Universe, t: TypeId, meth: SymbolId) -> Vec<SymbolId> {
    let mut found = Vec::new();
    u.map_type(t, &mut |u, p| {
        if let Type::Sym(_, s) = *u.types().get(p)
            && u.symbol(s).kind.is_type()
            && crate::sealed::is_contained_in(u, s, meth)
            && !found.contains(&s)
        {
            found.push(s);
        }
        None
    });
    found.reverse();
    found
}

/// Type `t` with its aliases expanded, part by part.
pub fn dealiased(u: &mut Universe, t: TypeId) -> TypeId {
    u.map_type(t, &mut |u, p| {
        let d = u.dealias(p);
        (d != p).then(|| dealiased(u, d))
    })
}

/// Accessor type `info` with its receiver parameter named `x$0`.
pub fn with_receiver_named(u: &mut Universe, info: TypeId) -> TypeId {
    match u.types().get(info).clone() {
        Type::Poly { binder, params, result } => {
            let result = with_receiver_named(u, result);
            u.intern_type(Type::Poly { binder, params, result })
        }
        Type::Method { binder, mut params, result, implicit, contextual } => {
            params[0].0 = "x$0".to_owned();
            u.intern_type(Type::Method { binder, params, result, implicit, contextual })
        }
        _ => info,
    }
}

fn refers_to(u: &mut Universe, t: TypeId, locals: &[SymbolId]) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, p| {
        found |= matches!(*u.types().get(p), Type::Sym(_, s) if locals.contains(&s));
        None
    });
    found
}
