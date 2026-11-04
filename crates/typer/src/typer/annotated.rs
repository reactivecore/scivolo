// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Annotated expressions `e: @a`, as `e: T @a`, and annotated types (dotty's `typedAnnotated`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{Attrs, NodeId};
use tasty::{Type, TypeId};

/// The meta-annotations of `scala.annotation.meta` that choose the definitions a field's
/// annotation goes to.
const ACCESSOR_METAS: [&str; 4] = ["field", "getter", "setter", "param"];

impl Typer<'_> {
    pub fn typed_annotated(&mut self, id: NodeId, arg: NodeId, annot: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let annot = self.typed(annot, Pt::Any, ctx);
        let annot_tpe = self.tpd.tpe(annot);
        if self.u.class_symbol(annot_tpe).is_none() {
            return self.unsupported(id, "annotation of an unknown class");
        }
        let arg = self.typed(arg, pt, ctx);
        let tpe = self.tpd.tpe(arg);
        let arg_tpe = match self.tpd.tree(arg) {
            T::Bind { .. } => self.u.widen(tpe),
            _ => self.widen_if_unstable(tpe),
        };
        let expr = match *self.tpd.tree(arg) {
            T::Typed { expr, tpt }
                if matches!(self.u.types().get(self.tpd.tpe(tpt)), Type::Annotated(..)) =>
            {
                expr
            }
            _ => arg,
        };
        let annotated = self.u.intern_type(Type::Annotated(arg_tpe, annot_tpe));
        let span = self.ast.span(id);
        let tpt = self.type_tree(annotated, self.tpd.span(expr).end_pos(), false);
        self.tpd.node(T::Typed { expr, tpt }, span, Some(annotated), None)
    }

    /// Annotated type `id`, `arg @annot`; an annotation meta-annotated `@annot` refused unless
    /// it is `deprecated`.
    pub fn typed_annotated_type(&mut self, id: NodeId, arg: NodeId, annot: NodeId, ctx: Ctx) -> TNodeId {
        let arg = self.typed_type(arg, ctx);
        let retains = self.ast.attrs(annot).contains(Attrs::RETAINS_ANNOT);
        let annot_ctx = if retains { ctx.with_mode(ctx.mode.with(Mode::IN_CAPTURE_SET)) } else { ctx };
        let annot = self.typed(annot, Pt::Any, annot_ctx);
        let annot_tpe = self.tpd.tpe(annot);
        let Some(annot_class) = self.u.class_symbol(annot_tpe) else {
            return self.unsupported(id, "annotated type");
        };
        let arg_class = self.u.class_symbol(self.tpd.tpe(arg));
        let meta = self.u.full_name(annot_class);
        if meta.strip_prefix("scala.annotation.meta.").is_some_and(|m| !ACCESSOR_METAS.contains(&m))
            && arg_class.is_none_or(|c| self.u.full_name(c) != "scala.deprecated")
        {
            return self.unsupported(id, "meta-annotated annotation other than deprecated");
        }
        let tpe = self.u.intern_type(Type::Annotated(self.tpd.tpe(arg), annot_tpe));
        self.tpd.node(T::Annotated { arg, annot }, self.ast.span(id), Some(tpe), None)
    }

    /// `t` without its annotations (dotty's `stripped`).
    pub fn stripped(&self, mut t: TypeId) -> TypeId {
        while let Type::Annotated(inner, _) = *self.u.types().get(t) {
            t = inner;
        }
        t
    }

    /// `t` widened as a match selector: a singleton, annotated or not, to its underlying type
    /// without annotations (dotty's `widen`).
    pub fn widened_selector(&mut self, t: TypeId) -> TypeId {
        let stripped = self.stripped(t);
        if self.is_singleton(stripped) { self.u.widen(stripped) } else { t }
    }

    /// `t` without its `@unchecked` annotation (dotty's `dropAnnot(defn.UncheckedAnnot)`).
    pub fn without_unchecked(&mut self, t: TypeId) -> TypeId {
        self.without_annotation(t, "scala.unchecked")
    }

    /// `t` without its annotation of class `name` (dotty's `dropAnnot`).
    pub fn without_annotation(&mut self, t: TypeId, name: &str) -> TypeId {
        let Type::Annotated(inner, annot) = *self.u.types().get(t) else { return t };
        if self.u.class_symbol(annot).is_some_and(|c| self.u.full_name(c) == name) {
            return inner;
        }
        let inner = self.without_annotation(inner, name);
        self.u.intern_type(Type::Annotated(inner, annot))
    }
}
