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

    /// The element type `T` of repeated parameter type `p`, `Seq[T] @Repeated`.
    pub fn repeated_elem(&mut self, p: TypeId) -> Option<TypeId> {
        let Type::Annotated(seq, annot) = self.u.types().get(p).clone() else { return None };
        if !self.is_repeated_annot(annot) {
            return None;
        }
        match self.u.types().get(seq) {
            Type::Applied(_, args) if args.len() == 1 => Some(args[0]),
            _ => None,
        }
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
        let class = self.repeated_class();
        let class = crate::defn::type_ref(self.u, class);
        let repeated = self.u.intern_type(Type::Applied(class, vec![elem]));
        let tpt = self.type_tree(repeated, span.end_pos(), true);
        self.tpd.node(T::Typed { expr: seq, tpt }, span, Some(repeated), None)
    }
}
