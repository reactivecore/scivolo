// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Repeated parameters `T*`, typed `Seq[T] @Repeated`, and the arguments passed to them as
//! `SeqLiteral`s typed `<repeated>[T]` (dotty's `typedArgs` for `RepeatedParamClass`).

use super::Typer;
use crate::defn::entered;
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
            Type::Annotated(t, _) if self.is_inline_param_type(p) => self.repeated_elem(t),
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
        match self.u.types().get(info).clone() {
            Type::Applied(_, args) if args.len() == 1 => self.repeated_type(args[0]),
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
        let (Some(seq_class), Some(array_class)) = (
            self.resolve_full_name("scala.collection.immutable.Seq", true),
            self.resolve_full_name("scala.Array", true),
        ) else {
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
        let converted = self.converted_result(expr);
        let expr_tpe = self.tpd.tpe(expr);
        let null = self.resolve_full_name("scala.Null", true);
        if converted
            || self
                .u
                .class_symbol(expr_tpe)
                .is_some_and(|c| Some(c) == null || c == self.u.builtins().nothing)
        {
            return self.unsupported(a, "sequence argument of a conversion or a bottom type");
        }
        let widened = self.u.widen(expr_tpe);
        let class = self.u.class_symbol(widened);
        let from =
            if class.is_some_and(|c| self.u.derives_from(c, array_class)) { array_class } else { seq_class };
        let base = self.u.base_type(widened, from);
        let Some(Type::Applied(_, args)) = base.map(|b| self.u.types().get(b).clone()) else {
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

    /// Whether `tree`, or a result of it (a branch, a case, a block's expression), is an implicit
    /// conversion.
    fn converted_result(&self, tree: TNodeId) -> bool {
        match self.tpd.tree(tree).clone() {
            T::Apply { mut fun, .. } => {
                while let T::TypeApply { fun: f, .. } = *self.tpd.tree(fun) {
                    fun = f;
                }
                self.tpd.sym(fun).is_some_and(|s| self.u.symbol(s).flags.has(tasty::tags::IMPLICIT))
            }
            T::If { thenp, elsep, .. } => self.converted_result(thenp) || self.converted_result(elsep),
            T::Block { expr, .. } | T::Typed { expr, .. } => self.converted_result(expr),
            T::Match { cases, .. } => self.converted_case(cases),
            T::Try { expr, cases, .. } => self.converted_result(expr) || self.converted_case(cases),
            _ => false,
        }
    }

    fn converted_case(&self, cases: crate::tpd::List) -> bool {
        self.tpd.items(cases).iter().any(|&c| match *self.tpd.tree(c) {
            T::CaseDef { body, .. } => self.converted_result(body),
            _ => false,
        })
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
        let Some(seq) = self.resolve_full_name("scala.collection.immutable.Seq", true) else { return };
        let elem = self.tpd.tpe(elemtpt);
        let seq = crate::defn::type_ref(self.u, seq);
        self.tpd.get_mut(tree).tpe = Some(self.u.intern_type(Type::Applied(seq, vec![elem])));
    }
}
