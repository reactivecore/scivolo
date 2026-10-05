//! Repeated parameters `T*`, typed `Seq[T] @Repeated`, and the arguments passed to them as
//! `SeqLiteral`s typed `<repeated>[T]` (dotty's `typedArgs` for `RepeatedParamClass`).

use super::Typer;
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
    /// as TASTy and parameter type trees give it.
    pub fn repeated_elem(&mut self, p: TypeId) -> Option<TypeId> {
        match self.u.types().get(p).clone() {
            Type::Annotated(seq, annot) if self.is_repeated_annot(annot) => match self.u.types().get(seq) {
                Type::Applied(_, args) if args.len() == 1 => Some(args[0]),
                _ => None,
            },
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
                let params = params
                    .into_iter()
                    .map(|(n, p)| match self.repeated_elem(p) {
                        Some(elem) => (n, self.repeated_type(elem)),
                        None => (n, p),
                    })
                    .collect();
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
        match self.u.types().get(info).clone() {
            Type::Applied(_, args) if args.len() == 1 => self.repeated_type(args[0]),
            _ => info,
        }
    }

    /// `<repeated>[elem]`.
    fn repeated_type(&mut self, elem: TypeId) -> TypeId {
        let class = self.repeated_class();
        let class = crate::defn::type_ref(self.u, class);
        self.u.intern_type(Type::Applied(class, vec![elem]))
    }

    /// dotty's `scala.<repeated>`, defined on first use.
    fn repeated_class(&mut self) -> SymbolId {
        if let Some(c) = self.syms.repeated_class {
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

    /// Sequence argument `a`, `xs*`, passed to a repeated parameter of element type `elem`:
    /// `xs` typed as a `Seq[elem]`, ascribed `<repeated>[elem]`.
    pub fn sequence_arg(&mut self, a: syntax::ast::NodeId, elem: TypeId, ctx: crate::ctx::Ctx) -> TNodeId {
        let syntax::ast::Tree::Typed { expr, tpt } = *self.ast.tree(a) else {
            return self.unsupported(a, "sequence argument");
        };
        let Some(seq) = self.resolve_full_name("scala.collection.immutable.Seq", true) else {
            return self.unsupported(a, "sequence argument");
        };
        let seq = crate::defn::type_ref(self.u, seq);
        let seq = self.u.intern_type(Type::Applied(seq, vec![elem]));
        let expr = self.typed(expr, super::Pt::Type(seq), ctx);
        let repeated = self.repeated_type(elem);
        let tpt = self.type_tree(repeated, self.ast.span(tpt), false);
        self.tpd.node(T::Typed { expr, tpt }, self.ast.span(a), Some(repeated), None)
    }

    /// Arguments `elems` of element type `elem` passed to a repeated parameter, at `span` if
    /// there are none: `elems: <repeated>[elem]`.
    pub fn seq_literal(&mut self, elems: &[TNodeId], elem: TypeId, span: Span) -> TNodeId {
        let span = match (elems.first(), elems.last()) {
            (Some(&a), Some(&b)) => {
                Span::new(self.tpd.span(a).start(), self.tpd.span(b).end(), self.tpd.span(a).start())
            }
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
}
