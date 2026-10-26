// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Dynamic.scala
//! Calls of structural methods, applied like methods and then rewritten to `applyDynamic`
//! (dotty's `handleStructural` for method types).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// `qual.name` for structural method `name` of type `method`: a selection without a symbol
    /// that `structural_call` rewrites once it is applied.
    pub fn structural_method(&mut self, qual: TNodeId, name: TName, method: TypeId, span: Span) -> TNodeId {
        let select = self.tpd.node(T::Select { qualifier: qual, name }, span, Some(method), None);
        self.structural_selects.insert(select);
        select
    }

    /// `tree` if it is no fully applied structural method; else
    /// `qual.applyDynamic("name", classOf[P1], …)(args…)` cast to its type, the arguments of all
    /// its clauses flattened (dotty's `handleStructural`).
    pub fn structural_call(&mut self, tree: TNodeId, ctx: Ctx) -> TNodeId {
        if self.structural_selects.is_empty() {
            return tree;
        }
        let Some(tpe) = self.tpd.get(tree).tpe else { return tree };
        if matches!(self.u.types().get(tpe), Type::Method { .. } | Type::Poly { .. }) {
            return tree;
        }
        let (mut f, mut clauses) = (tree, Vec::new());
        while let T::Apply { fun, args } = *self.tpd.tree(f) {
            clauses.push(self.tpd.items(args).to_vec());
            f = fun;
        }
        if !self.structural_selects.remove(&f) {
            return tree;
        }
        let T::Select { qualifier: qual, name } = *self.tpd.tree(f) else { return tree };
        let span = self.tpd.span(tree);
        let mut method = self.tpd.tpe(f);
        let mut params = Vec::new();
        while let Type::Method { binder, params: ps, result, .. } = self.u.types().get(method).clone() {
            for (_, p) in ps {
                let owner = self.param_owner(p);
                let p = self.without_false_deps(p, binder, owner);
                if self.mentions_params(p, binder, owner) || self.repeated_elem(p).is_some() {
                    return self
                        .unsupported_tree(span, "structural method of dependent or repeated parameters");
                }
                params.push(p);
            }
            method = result;
        }
        clauses.reverse();
        let args: Vec<TNodeId> = clauses.concat();
        if args.len() != params.len() {
            return self.unsupported_tree(span, "structural call with lifted or missing arguments");
        }
        let qual = match self.selectable_qual(qual, ctx) {
            Ok(q) => q,
            Err(what) => return self.unsupported_tree(span, what),
        };
        let qual_tpe = self.zonk(self.tpd.tpe(qual));
        let with_classes = self.takes_class_ofs(qual_tpe);
        let class_ofs = match with_classes {
            true => match self.class_ofs(&params, self.tpd.span(f)) {
                Some(class_ofs) => class_ofs,
                None => {
                    return self
                        .unsupported_tree(span, "structural parameter of an array, value class or no class");
                }
            },
            false => Vec::new(),
        };
        let text = self.text(name.name).to_owned();
        let (fun, literal) = self.structural_fun(qual, "applyDynamic", &text, self.tpd.span(f));
        let first: Vec<NodeId> = std::iter::once(literal).chain(class_ofs).collect();
        let (inner, _) = self.applied(fun, &first);
        let spliced: Vec<NodeId> = args.into_iter().map(|a| self.spliced(a)).collect();
        let (outer, _) = self.applied(inner, &spliced);
        let call = self.typed(outer, Pt::Any, ctx);
        if with_classes {
            self.written_class_ofs(call);
        }
        self.structural_cast(span, (qual, call), tpe, ctx)
    }

    /// The method owning a parameter `t` refers to, if any.
    fn param_owner(&mut self, t: TypeId) -> Option<tasty::SymbolId> {
        let mut owner = None;
        self.u.map_type(t, &mut |u, part| {
            if let Type::Sym(_, s) = *u.types().get(part)
                && u.symbol(s).kind == tasty::Kind::Param
            {
                owner = owner.or(u.symbol(s).owner);
            }
            None
        });
        owner
    }

    /// Refuses the structural methods no application rewrote.
    pub fn check_structural_calls(&mut self) {
        let spans: Vec<Span> = self.structural_selects.iter().map(|&s| self.tpd.span(s)).collect();
        if let Some(&span) = spans.iter().min_by_key(|s| (s.start(), s.end())) {
            self.unsupported_at(span, "structural method applied through another tree");
        }
    }

    /// Whether the `applyDynamic` of `qual_tpe` takes the parameter classes after the name,
    /// `(name: String, paramClasses: Class[?]*)` (dotty's `addClassOfs`).
    fn takes_class_ofs(&mut self, qual_tpe: TypeId) -> bool {
        let class = self.resolve_full_name("java.lang.Class", true);
        let members = self.type_members(qual_tpe, "applyDynamic");
        members.into_iter().any(|m| {
            let info = self.u.member_info(qual_tpe, m);
            let Type::Method { params, .. } = self.u.types().get(info).clone() else { return false };
            let elem = params.get(1).filter(|_| params.len() == 2).and_then(|&(_, p)| self.repeated_elem(p));
            elem.is_some_and(|e| self.u.class_symbol(e) == class)
        })
    }

    /// The untyped `classOf[C]` of the classes parameter types `params` erase to; `None` if one
    /// is an array, a value class or erases to no class.
    fn class_ofs(&mut self, params: &[TypeId], at: Span) -> Option<Vec<NodeId>> {
        let at = at.end_pos();
        let class_class = self.resolve_full_name("java.lang.Class", true)?;
        let mut elems = Vec::new();
        for &p in params {
            let widened = self.u.widen(p);
            let dealiased = self.u.dealias(widened);
            if self.class_args(dealiased, "scala.Array").is_some() || self.is_value_class_type(dealiased) {
                return None;
            }
            let class = self.u.erased_class(dealiased)?;
            let erased = crate::defn::type_ref(self.u, class);
            let tpe = crate::defn::applied(self.u, class_class, vec![erased]);
            let literal = self.tpd.node(T::ClassLiteral { tpe: erased }, at, Some(tpe), None);
            elems.push(self.spliced(literal));
        }
        Some(elems)
    }

    /// Marks the element type of the `classOf` sequence of `applyDynamic` call `call` written,
    /// as dotty's `addClassOfs` gives it.
    fn written_class_ofs(&mut self, call: TNodeId) {
        let mut f = call;
        while let T::Apply { fun, args } = *self.tpd.tree(f) {
            if let [_, seq] = *self.tpd.items(args)
                && let T::Typed { expr, .. } = *self.tpd.tree(seq)
                && let T::SeqLiteral { elemtpt, .. } = *self.tpd.tree(expr)
                && !matches!(self.tpd.tree(fun), T::Apply { .. })
            {
                self.tpd.get_mut(elemtpt).tree = T::TypeTree { inferred: false };
            }
            f = fun;
        }
    }

    /// The untyped `qual.selector` at `span` and the literal `"name"` after it (dotty's
    /// `structuralCall`).
    pub fn structural_fun(
        &mut self,
        qual: TNodeId,
        selector: &str,
        name: &str,
        span: Span,
    ) -> (NodeId, NodeId) {
        let qualifier = self.spliced(qual);
        let selector = TName { name: self.name(selector), is_type: false };
        let fun = self.ast.node(Tree::Select { qualifier, name: selector });
        self.ast.with_span(fun, span);
        (fun, self.string_literal(&tasty::encode(name), span.end_pos()))
    }
}
