// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Dynamic.scala
//! Structural refinements `{ val a: T; def m(x: A): R }` and selections of their members
//! (dotty's `handleStructural`).

use super::implicits::conversions::Goal;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags::{GIVEN, IMPLICIT, MUTABLE};
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// The name and info of term refinement `r`: a value's type, a parameterless method's
    /// by-name result, a method type; `Err` for one that is not a plain value or method.
    pub fn term_refinement(&mut self, r: NodeId, ctx: Ctx) -> Result<(String, TypeId), &'static str> {
        let (name, clauses, tpt, mods) = match *self.ast.tree(r) {
            Tree::ValDef { name, tpt, mods, .. } => (name, None, tpt, mods),
            Tree::DefDef { name, paramss, tpt, mods, .. } => (name, Some(paramss), tpt, mods),
            _ => return Err("structural refinement"),
        };
        if self.ast.modifiers(mods).flags.is_one_of(MUTABLE.or(GIVEN).or(IMPLICIT)) {
            return Err("structural refinement with modifiers");
        }
        let text = self.text(name.name).to_owned();
        let result = self.typed_type(tpt, ctx);
        let mut info = self.tpd.tpe(result);
        if matches!(self.u.types().get(info), Type::Error) {
            return Err("structural refinement of an unsupported type");
        }
        let Some(clauses) = clauses else { return Ok((text, info)) };
        let clauses = self.ast.clause_items(clauses).to_vec();
        if clauses.is_empty() {
            return Ok((text, self.u.intern_type(Type::ByName(info))));
        }
        for &clause in clauses.iter().rev() {
            let mut params = Vec::new();
            for &p in self.ast.items(clause).to_vec().iter() {
                let Tree::ValDef { name, tpt, mods, .. } = *self.ast.tree(p) else {
                    return Err("polymorphic structural refinement");
                };
                if self.ast.modifiers(mods).flags.is_one_of(GIVEN.or(IMPLICIT)) {
                    return Err("structural refinement with a context parameter");
                }
                if matches!(self.ast.tree(tpt), Tree::ByNameTypeTree { .. }) {
                    return Err("structural refinement with a by-name parameter");
                }
                let typed = self.typed_type(tpt, ctx);
                let tpe = self.tpd.tpe(typed);
                if matches!(self.u.types().get(tpe), Type::Error) {
                    return Err("structural refinement of an unsupported type");
                }
                params.push((self.text(name.name).to_owned(), tpe));
            }
            let binder = self.u.fresh_binder();
            let method = Type::Method { binder, params, result: info, implicit: false, contextual: false };
            info = self.u.intern_type(method);
        }
        Ok((text, info))
    }

    /// Selection `id` of member `name` of `qual`, which only a term refinement of its type `pre`
    /// declares: `(qual: Selectable).selectDynamic("name").$asInstanceOf$[T]`; `None` if no
    /// refinement declares it.
    pub fn structural_select(
        &mut self,
        id: NodeId,
        (qual, pre): (TNodeId, TypeId),
        name: TName,
        pt: Pt,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        let text = self.text(name.name).to_owned();
        if name.is_type {
            return None;
        }
        let info = self.u.term_refinement_named(pre, &text)?;
        let method = match self.u.types().get(info).clone() {
            Type::Poly { .. } => return Some(self.unsupported(id, "polymorphic structural method")),
            Type::Method { .. } if !matches!(pt, Pt::Fun(_)) => {
                return Some(self.unsupported(id, "structural method value"));
            }
            Type::Method { .. } => Some(info),
            _ => None,
        };
        let tpe = self.u.intern_type(Type::TermRef(pre, text.clone()));
        let selectable = self.u.class_ref("scala.Selectable");
        let qual_tpe = self.zonk(self.tpd.tpe(qual));
        let qual = if self.u.is_subtype(qual_tpe, selectable) {
            qual
        } else {
            match self.converted(qual, Goal::Conform(selectable), ctx) {
                Ok(Some(q)) => q,
                Ok(None) => {
                    return Some(self.unsupported(id, "structural selection without reflectiveSelectable"));
                }
                Err(what) => return Some(self.unsupported(id, what)),
            }
        };
        let span = self.ast.span(id);
        if let Some(method) = method {
            return Some(self.pending_structural(qual, text, span, method));
        }
        let (fun, literal) = self.structural_fun(qual, "selectDynamic", &text, span);
        let (apply, args) = self.applied(fun, &[literal]);
        let call = self.typed_apply(apply, fun, args, Pt::Any, ctx);
        Some(self.structural_cast(id, call, tpe))
    }

    /// Structural call `id` of method `name` of type `method` on `qual` to `args` (dotty's
    /// `handleStructural`): `qual.applyDynamic("name", classOf[P1], …)(args).$asInstanceOf$[R]`.
    pub fn structural_apply(
        &mut self,
        id: NodeId,
        (qual, name, span): (TNodeId, &str, Span),
        method: TypeId,
        args: &[NodeId],
        ctx: Ctx,
    ) -> TNodeId {
        let Type::Method { params, result, .. } = self.u.types().get(method).clone() else {
            return self.unsupported(id, "structural method call");
        };
        if matches!(self.u.types().get(result), Type::Method { .. }) {
            return self.unsupported(id, "structural call with several argument lists");
        }
        let reflective = self.u.class_ref("scala.reflect.Selectable");
        let qual_tpe = self.zonk(self.tpd.tpe(qual));
        if !self.u.is_subtype(qual_tpe, reflective) {
            return self.unsupported(id, "structural call on a Selectable other than reflect.Selectable");
        }
        let plain =
            |s: &Self, a: NodeId| !matches!(s.ast.tree(a), Tree::NamedArg { .. }) && !s.is_sequence_arg(a);
        if args.len() != params.len() || !args.iter().all(|&a| plain(self, a)) {
            return self.unsupported(id, "structural call with named, repeated or missing arguments");
        }
        let mut class_ofs = Vec::new();
        for &(_, p) in &params {
            let Some(c) = self.structural_class_of(p, span) else {
                return self.unsupported(id, "structural parameter of an array, value class or no class");
            };
            class_ofs.push(c);
        }
        let mut typed_args = Vec::new();
        for (&a, &(_, p)) in args.iter().zip(&params) {
            let typed = self.typed(a, Pt::Type(p), ctx);
            if self.is_error(typed) {
                return typed;
            }
            typed_args.push(self.spliced(typed));
        }
        let (fun, literal) = self.structural_fun(qual, "applyDynamic", name, span);
        let first: Vec<NodeId> = std::iter::once(literal).chain(class_ofs).collect();
        let (inner, _) = self.applied(fun, &first);
        let (outer, list) = self.applied(inner, &typed_args);
        let call = self.typed_apply(outer, inner, list, Pt::Any, ctx);
        self.structural_cast(id, call, result)
    }

    /// The untyped `qual.selector` at `span` and the literal `"name"` after it (dotty's
    /// `structuralCall`).
    fn structural_fun(&mut self, qual: TNodeId, selector: &str, name: &str, span: Span) -> (NodeId, NodeId) {
        let qualifier = self.spliced(qual);
        let selector = TName { name: self.name(selector), is_type: false };
        let fun = self.ast.node(Tree::Select { qualifier, name: selector });
        self.ast.with_span(fun, span);
        (fun, self.string_literal(&tasty::encode(name), span.end_pos()))
    }

    /// The untyped `classOf[C]` of the class parameter type `p` erases to; `None` for an array, a
    /// value class or a type that erases to no class.
    fn structural_class_of(&mut self, p: TypeId, at: Span) -> Option<NodeId> {
        let widened = self.u.widen(p);
        let dealiased = self.u.dealias(widened);
        if self.class_args(dealiased, "scala.Array").is_some() || self.is_value_class_type(dealiased) {
            return None;
        }
        let class = self.u.erased_class(dealiased)?;
        let erased = crate::defn::type_ref(self.u, class);
        let class_class = self.resolve_full_name("java.lang.Class", true)?;
        let tpe = crate::defn::applied(self.u, class_class, vec![erased]);
        let literal = self.tpd.node(T::ClassLiteral { tpe: erased }, at.end_pos(), Some(tpe), None);
        Some(self.spliced(literal))
    }

    /// Whether `t` widens to a value class or a by-name one.
    fn is_value_class_type(&mut self, t: TypeId) -> bool {
        let widened = self.u.widen(t);
        let value = if let Type::ByName(r) = *self.u.types().get(widened) { r } else { widened };
        self.u.value_class(value).is_some()
    }

    /// `call.$asInstanceOf$[tpe]`, or `call` if it is an error; refused for a value class.
    fn structural_cast(&mut self, id: NodeId, call: TNodeId, tpe: TypeId) -> TNodeId {
        if self.is_error(call) {
            return call;
        }
        if self.is_value_class_type(tpe) {
            return self.unsupported(id, "structural member of a value class type");
        }
        self.cast_to_captured(call, tpe).unwrap_or_else(|what| self.unsupported(id, what))
    }
}
