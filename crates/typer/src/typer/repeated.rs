// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Repeated parameters `T*`, typed `Seq[T] @Repeated`, and the arguments passed to them as
//! `SeqLiteral`s typed `<repeated>[T]` (dotty's `typedArgs` for `RepeatedParamClass`).

use super::Typer;
use super::type_ops::TypeOps;
use crate::defn::{ARRAY, IMMUTABLE_SEQ, INLINE_PARAM};
use crate::special::entered;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Type, TypeId};

const REPEATED_ANNOT: &str = "scala.annotation.internal.Repeated";

impl Typer<'_> {
    /// Whether `annot`, an annotation's class type, marks a repeated parameter.
    pub fn is_repeated_annot(&mut self, annot: TypeId) -> bool {
        self.u.class_symbol(annot).is_some_and(|c| self.u.full_name(c) == REPEATED_ANNOT)
    }

    /// The element type `T` of repeated parameter type `p`: `<repeated>[T]`, or `Seq[T] @Repeated`
    /// as TASTy and parameter type trees give it, also annotated `@InlineParam`.
    pub fn repeated_elem(&mut self, p: TypeId) -> Option<TypeId> {
        match self.u.types().get(p).clone() {
            Type::Annotated(seq, annot) if self.is_repeated_annot(annot) => match self.u.types().get(seq) {
                Type::Applied(_, args) if args.len() == 1 => Some(args[0]),
                _ => None,
            },
            Type::Annotated(t, _) if self.types().is_inline_param_type(p) => self.repeated_elem(t),
            Type::Applied(c, args) if args.len() == 1 => {
                let class = self.u.class_symbol(c);
                (class.is_some() && class == self.syms.repeated_class).then_some(args[0])
            }
            _ => None,
        }
    }

    /// Method type `t` with its repeated parameters typed `<repeated>[T]`, in each clause.
    pub fn with_repeated_class(&mut self, t: TypeId) -> TypeId {
        match self.u.types().get(t).clone() {
            Type::Method { binder, params, result, implicit, contextual } => {
                let params =
                    params.into_iter().map(|(n, p)| (n, self.with_repeated_param_class(p))).collect();
                let result = self.with_repeated_class(result);
                self.u.intern_type(Type::Method { binder, params, result, implicit, contextual })
            }
            Type::Poly { binder, params, result } => {
                let result = self.with_repeated_class(result);
                self.u.intern_type(Type::Poly { binder, params, result })
            }
            _ => t,
        }
    }

    /// The type of script parameter `p` in its method's type: `<repeated>[T]` for a repeated one.
    pub fn param_type(&mut self, p: SymbolId) -> TypeId {
        let info = self.u.info(p);
        if !self.syms.repeated_params.contains_key(&p) {
            return info;
        }
        match *self.u.types().get(info) {
            Type::Applied(_, ref args) if args.len() == 1 => self.repeated_type(args[0]),
            _ => info,
        }
    }

    /// `<repeated>[elem]`.
    pub fn repeated_type(&mut self, elem: TypeId) -> TypeId {
        let class = self.repeated_class();
        let class = crate::defn::type_ref(self.u, class);
        self.u.intern_type(Type::Applied(class, vec![elem]))
    }

    /// dotty's `scala.<repeated>`, defined on first use.
    fn repeated_class(&mut self) -> SymbolId {
        let scala = self.defn.scala;
        if let Some(c) =
            self.syms.repeated_class.or_else(|| entered(self.u, scala, "<repeated>", Kind::Class))
        {
            self.syms.repeated_class = Some(c);
            return c;
        }
        let c = self.syms.define(
            self.u,
            "<repeated>",
            Kind::Class,
            self.defn.scala,
            syntax::ast::flags::SYNTHETIC,
        );
        self.syms.repeated_class = Some(c);
        c
    }

    /// `sequence_arg` of `a`, or `pre` if overload resolution typed it with elements of `elem`.
    pub fn pretyped_sequence_arg(
        &mut self,
        a: syntax::ast::NodeId,
        pre: Option<TNodeId>,
        elem: TypeId,
        ctx: crate::ctx::Ctx,
    ) -> TNodeId {
        let Some(pre) = pre else { return self.sequence_arg(a, Some(elem), ctx) };
        match self.repeated_elem(self.tpd.tpe(pre)) {
            Some(e) if self.conforms(e, elem) => pre,
            _ => self.unsupported(a, "sequence argument of an overload"),
        }
    }

    /// Sequence argument `a`, `xs*`, passed to a repeated parameter of element type `elem`:
    /// `xs` typed as a `Seq[elem] | Array[? <: elem]`, without an expected type if no `elem`,
    /// ascribed `<repeated>[T]` with `T` its element type.
    pub fn sequence_arg(
        &mut self,
        a: syntax::ast::NodeId,
        elem: Option<TypeId>,
        ctx: crate::ctx::Ctx,
    ) -> TNodeId {
        let syntax::ast::Tree::Typed { expr, tpt } = *self.ast.tree(a) else {
            return self.unsupported(a, "sequence argument");
        };
        let (Some(seq_class), Some(array_class)) =
            (self.resolve_full_name(IMMUTABLE_SEQ, true), self.resolve_full_name(ARRAY, true))
        else {
            return self.unsupported(a, "sequence argument");
        };
        let pt = elem.map_or(super::Pt::Any, |elem| {
            let seq = crate::defn::type_ref(self.u, seq_class);
            let seq = self.u.intern_type(Type::Applied(seq, vec![elem]));
            let array = crate::defn::type_ref(self.u, array_class);
            let below = self.u.intern_type(Type::Bounds(tasty::NOTHING, elem));
            let array = self.u.intern_type(Type::Applied(array, vec![below]));
            super::Pt::Type(self.u.union(seq, array, false))
        });
        let expr = self.typed(expr, pt, ctx);
        let expr_tpe = self.tpd.tpe(expr);
        let null = self.resolve_full_name("scala.Null", true);
        if self.u.class_symbol(expr_tpe).is_some_and(|c| Some(c) == null || c == self.u.builtins().nothing) {
            return self.unsupported(a, "sequence argument of a bottom type");
        }
        let widened = self.u.widen(expr_tpe);
        let class = self.u.class_symbol(widened);
        let from =
            if class.is_some_and(|c| self.u.derives_from(c, array_class)) { array_class } else { seq_class };
        let base = self.u.base_type(widened, from);
        let Some((_, args)) = base.and_then(|b| self.u.applied(b)) else {
            return self.unsupported(a, "sequence argument");
        };
        let arg = match *self.u.types().get(args[0]) {
            Type::Bounds(_, hi) => hi,
            _ => args[0],
        };
        let repeated = self.repeated_type(arg);
        let tpt = self.type_tree(repeated, self.ast.span(tpt), false);
        self.tpd.node(T::Typed { expr, tpt }, self.ast.span(a), Some(repeated), None)
    }

    /// Arguments `elems` of element type `elem` passed to a repeated parameter, at `span` if
    /// there are none: `elems: <repeated>[elem]`.
    pub fn seq_literal(&mut self, elems: &[TNodeId], elem: TypeId, span: Span) -> TNodeId {
        let start = elems.iter().map(|&a| self.tpd.span(a).start()).min();
        let end = elems.iter().map(|&a| self.tpd.span(a).end()).max();
        let span = match (start, end) {
            (Some(start), Some(end)) => Span::new(start, end, start),
            _ => span,
        }
        .to_synthetic();
        let elemtpt = self.type_tree(elem, span.end_pos(), true);
        let elems = self.tpd.list(elems);
        let seq = self.tpd.node(T::SeqLiteral { elems, elemtpt }, span, None, None);
        let repeated = self.repeated_type(elem);
        let tpt = self.type_tree(repeated, span.end_pos(), true);
        self.tpd.node(T::Typed { expr: seq, tpt }, span, Some(repeated), None)
    }

    /// Gives `SeqLiteral` `tree` without a type its type `Seq[elem]`.
    pub fn type_seq_literal(&mut self, tree: TNodeId) {
        let T::SeqLiteral { elemtpt, .. } = *self.tpd.tree(tree) else { return };
        if self.tpd.get(tree).tpe.is_some() {
            return;
        }
        let Some(seq) = self.resolve_full_name(IMMUTABLE_SEQ, true) else { return };
        let elem = self.tpd.tpe(elemtpt);
        let seq = crate::defn::type_ref(self.u, seq);
        self.tpd.get_mut(tree).tpe = Some(self.u.intern_type(Type::Applied(seq, vec![elem])));
    }
}

impl TypeOps<'_> {
    /// Tells whether parameter type `p` is annotated `@InlineParam`, as the type of an
    /// `inline` parameter of an inline method is.
    pub fn is_inline_param_type(&mut self, p: TypeId) -> bool {
        let Type::Annotated(_, annot) = *self.u.types().get(p) else { return false };
        self.u.class_symbol(annot).is_some_and(|c| self.u.full_name(c) == INLINE_PARAM)
    }
}
