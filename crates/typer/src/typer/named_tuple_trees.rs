// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//! The trees of named tuple types, `NamedTuple[("a", "b"), (A, B)]` as dotty's `desugar.tuple`
//! writes them.

use super::small_tuples::MAX_TUPLE_ARITY;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::constant::Constant;
use syntax::ast::{Span, TName, Tree};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// The tree of named tuple type `tpe` at `span`, written with names `names` and value type
    /// trees `values`: `NamedTuple[TupleN[n1.type, …], TupleN[V1, …]]`; a type tree above 22
    /// elements.
    pub fn named_tuple_tree(
        &mut self,
        tpe: TypeId,
        names: &[String],
        values: &[TNodeId],
        span: Span,
    ) -> TNodeId {
        let Type::Applied(nt, ref args) = *self.u.types().get(tpe) else {
            return self.type_tree(tpe, span, false);
        };
        let [names_tpe, values_tpe] = args[..] else { return self.type_tree(tpe, span, false) };
        if values.len() > MAX_TUPLE_ARITY || values.is_empty() {
            return self.type_tree(tpe, span, false);
        }
        let start = span.start_pos().to_synthetic();
        let written = Span::new(span.start(), span.end(), span.start());
        let names_tree = self.names_tree(names_tpe, names, span);
        let first = self.tpd.span(values[0]).start_pos().to_synthetic();
        let values_tree = self.tuple_type_tree(values_tpe, first, values, written);
        let tpt = self.type_tree(nt, start, false);
        let args = self.tpd.list(&[names_tree, values_tree]);
        self.tpd.node(T::AppliedTypeTree { tpt, args }, span.to_synthetic(), Some(tpe), None)
    }

    /// Named tuple literal `values` with names `names` at `span`, typed in `ctx`:
    /// `NamedTuple.build[(n1.type, …)]()(values)`; `None` without the names' tuple class.
    pub fn named_tuple_build(
        &mut self,
        names: &[String],
        values: TNodeId,
        span: Span,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        let types: Vec<TypeId> = names.iter().map(|n| self.u.intern_type(Type::Str(n.clone()))).collect();
        let names_tpe = self.values_type(&types)?;
        let names_tree = self.names_tree(names_tpe, names, span);
        let start = span.start_pos().to_synthetic();
        let synthetic = span.to_synthetic();
        let module = TName { name: self.name("scala.NamedTuple"), is_type: false };
        let qualifier = self.ast.node(Tree::TypedSplice { sym: module });
        self.ast.with_span(qualifier, start);
        let name = TName { name: self.name("build"), is_type: false };
        let fun = self.ast.node(Tree::Select { qualifier, name });
        self.ast.with_span(fun, start);
        let targs = [self.spliced(names_tree)];
        let targs = self.ast.list(&targs);
        let fun = self.ast.node(Tree::TypeApply { fun, args: targs });
        self.ast.with_span(fun, synthetic);
        let none = self.ast.list(&[]);
        let fun = self.ast.node(Tree::Apply { fun, args: none });
        self.ast.with_span(fun, synthetic);
        let args = [self.spliced(values)];
        let args = self.ast.list(&args);
        let apply = self.ast.node(Tree::Apply { fun, args });
        self.ast.with_span(apply, synthetic);
        Some(self.typed(apply, Pt::Any, ctx))
    }

    /// `TupleN[n1.type, …]` of type `tpe` for names `names` written at `span`.
    fn names_tree(&mut self, tpe: TypeId, names: &[String], span: Span) -> TNodeId {
        let start = span.start_pos().to_synthetic();
        let written = Span::new(span.start(), span.end(), span.start());
        let mut trees = Vec::new();
        for n in names {
            let value = Constant::String(n.encode_utf16().collect());
            let literal = self.build().literal(value, start);
            let reference = self.tpd.tpe(literal);
            trees.push(self.tpd.node(
                T::SingletonTypeTree { reference: literal },
                written,
                Some(reference),
                None,
            ));
        }
        self.tuple_type_tree(tpe, start, &trees, span.to_synthetic())
    }

    /// `TupleN[args]` of type `tpe` at `span`, its constructor at `at`.
    fn tuple_type_tree(&mut self, tpe: TypeId, at: Span, args: &[TNodeId], span: Span) -> TNodeId {
        let tycon = match *self.u.types().get(tpe) {
            Type::Applied(tycon, _) => tycon,
            _ => tpe,
        };
        let tpt = self.type_tree(tycon, at, false);
        let args = self.tpd.list(args);
        self.tpd.node(T::AppliedTypeTree { tpt, args }, span, Some(tpe), None)
    }
}
