// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Repeated parameters `T*`, typed `Seq[T] @Repeated`, and the arguments passed to them as
//! `SeqLiteral`s typed `<repeated>[T]` (dotty's `typedArgs` for `RepeatedParamClass`).

use super::Typer;
use super::type_ops::TypeOps;
use crate::defn::{IMMUTABLE_SEQ, INLINE_PARAM, REPEATED_ANNOT};
use crate::special::entered;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether `annot`, an annotation's class type, marks a repeated parameter.
    pub fn is_repeated_annot(&mut self, annot: TypeId) -> bool {
        self.u.is_class_named(annot, REPEATED_ANNOT)
    }

    /// The element type `T` of repeated parameter type `p`: `<repeated>[T]`, or `Seq[T] @Repeated`
    /// as TASTy and parameter type trees give it, also annotated `@InlineParam`.
    pub fn repeated_elem(&mut self, p: TypeId) -> Option<TypeId> {
        let ty = *self.u.ty(p);
        match ty {
            Type::Annotated(seq, annot) if self.is_repeated_annot(annot) => match self.u.applied(seq) {
                Some((_, &[arg])) => Some(arg),
                _ => None,
            },
            Type::Annotated(t, _) if self.types().is_inline_param_type(p) => self.repeated_elem(t),
            Type::Applied(c, args) if self.u.types_of(args).len() == 1 => {
                let arg = self.u.types_of(args)[0];
                let class = self.u.class_symbol(c);
                (class.is_some() && class == self.syms.repeated_class).then_some(arg)
            }
            _ => None,
        }
    }

    /// Method type `t` with its repeated parameters typed `<repeated>[T]`, in each clause.
    pub fn with_repeated_class(&mut self, t: TypeId) -> TypeId {
        match *self.u.ty(t) {
            Type::Method { binder, params, result, implicit, contextual } => {
                let params: Vec<_> = (0..self.u.params_of(params).len())
                    .map(|i| {
                        let (n, p) = self.u.params_of(params)[i];
                        (n, self.with_repeated_param_class(p))
                    })
                    .collect();
                let result = self.with_repeated_class(result);
                self.u.intern_method(binder, &params, result, implicit, contextual)
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
        match self.u.applied(info) {
            Some((_, &[arg])) => self.repeated_type(arg),
            _ => info,
        }
    }

    /// `<repeated>[elem]`.
    pub fn repeated_type(&mut self, elem: TypeId) -> TypeId {
        let class = self.repeated_class();
        let class = crate::defn::type_ref(self.u, class);
        self.u.intern_applied(class, &[elem])
    }

    /// dotty's `scala.<repeated>`, defined on first use.
    fn repeated_class(&mut self) -> SymbolId {
        let scala = self.defn.scala;
        if let Some(c) = self
            .syms
            .repeated_class
            .or_else(|| entered(self.u, scala, names::nme::REPEATED_PARAM_CLASS, Kind::Class))
        {
            self.syms.repeated_class = Some(c);
            return c;
        }
        let c = self.syms.define(
            self.u,
            names::nme::REPEATED_PARAM_CLASS,
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
        let elem = self.tpd.tpe(elemtpt);
        self.tpd.get_mut(tree).tpe = self.seq_of(elem);
    }

    /// `scala.collection.immutable.Seq[elem]`.
    pub fn seq_of(&mut self, elem: TypeId) -> Option<TypeId> {
        let seq = self.resolve_full_name(IMMUTABLE_SEQ, true)?;
        let seq = crate::defn::type_ref(self.u, seq);
        Some(self.u.intern_applied(seq, &[elem]))
    }
}

impl TypeOps<'_> {
    /// Tells whether parameter type `p` is annotated `@InlineParam`, as the type of an
    /// `inline` parameter of an inline method is.
    pub fn is_inline_param_type(&mut self, p: TypeId) -> bool {
        let Some((_, annot)) = self.u.annotated(p) else { return false };
        self.u.is_class_named(annot, INLINE_PARAM)
    }
}
