// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Tuples of more than 22 elements (dotty's `typedTuple` above `MaxTupleArity`): expressions as
//! `TupleXXL(…)` cast to the `*:` chain of their element types, patterns as `TupleXXL(…)`
//! extractors, types as `*:` chains ending in `EmptyTuple`.

use super::widen_inferred::Widen;
use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, Span, TName, Tree};
use tasty::{Type, TypeId};

impl Typer<'_> {
    pub fn typed_tuple_xxl(&mut self, id: NodeId, trees: List, pt: Pt, ctx: Ctx) -> TNodeId {
        match self.named_elements(trees) {
            Some(Ok(_)) if ctx.mode.is(Mode::PATTERN) => {
                return self.typed_named_tuple_pattern(id, trees, pt, ctx);
            }
            Some(Ok(named)) => return self.typed_named_tuple(id, named, pt, ctx),
            Some(Err(what)) => return self.unsupported(id, what),
            None => {}
        }
        let elems = self.ast.items(trees).to_vec();
        if elems.len() <= super::small_tuples::MAX_TUPLE_ARITY {
            return self.unsupported(id, "tuple of at most 22 elements left by desugaring");
        }
        let span = self.ast.span(id);
        let module = TName { name: self.name("scala.runtime.TupleXXL"), is_type: false };
        let fun = self.ast.node(Tree::TypedSplice { sym: module });
        let first = self.ast.span(elems[0]).start_pos();
        self.ast.with_span(fun, first);
        let app = self.ast.node(Tree::Apply { fun, args: trees });
        self.ast.with_span(app, span);
        let pts = match pt {
            Pt::Type(t) => self.tuple_element_types(t),
            _ => None,
        };
        let pts = pts.filter(|p| p.len() == elems.len()).unwrap_or_else(|| vec![tasty::ANY; elems.len()]);
        let mut types = Vec::with_capacity(elems.len());
        for (&e, &p) in elems.iter().zip(&pts) {
            let typed = self.typed(e, Pt::Type(p), ctx);
            self.typed_ahead(e, typed, ctx);
            if !ctx.mode.is(Mode::PATTERN) {
                let tpe = self.tpd.tpe(typed);
                match self.widen_inferred(tpe, p, Widen::Unions) {
                    Ok(t) => types.push(t),
                    Err(what) => return self.unsupported(e, what),
                }
            }
        }
        if ctx.mode.is(Mode::PATTERN) {
            return self.typed(app, pt, ctx);
        }
        let Some(class) = self.resolve_full_name("scala.runtime.TupleXXL", true) else {
            return self.unsupported(id, "TupleXXL missing from the library");
        };
        let class = crate::defn::type_ref(self.u, class);
        let typed = self.typed(app, Pt::Type(class), ctx);
        match self.nested_pairs(&types) {
            Ok(to) => self.type_cast(typed, to),
            Err(what) => self.unsupported(id, what),
        }
    }

    /// Tuple type `id` of elements `trees` as `A *: B *: … *: EmptyTuple`.
    pub fn typed_tuple_type(&mut self, id: NodeId, trees: List, ctx: Ctx) -> TNodeId {
        let span = self.ast.span(id);
        match self.named_elements(trees) {
            Some(Ok(named)) => return self.typed_named_tuple_type(id, named, ctx),
            Some(Err(what)) => return self.unsupported(id, what),
            None => {}
        }
        let elems: Vec<TNodeId> =
            self.ast.items(trees).to_vec().iter().map(|&e| self.typed_type(e, ctx)).collect();
        let (Some(pair), Some(empty)) =
            (self.resolve_full_name("scala.*:", true), self.resolve_full_name("scala.EmptyTuple", false))
        else {
            return self.unsupported(id, "tuple classes missing from the library");
        };
        let pair = crate::defn::type_ref(self.u, pair);
        let empty = self.u.intern_type(Type::Sym(None, empty));
        let last = elems.last().map_or(span, |&e| self.tpd.span(e));
        let mut acc = self.type_tree(empty, last.end_pos(), false);
        for (i, &e) in elems.iter().enumerate().rev() {
            let elem_span = self.tpd.span(e);
            let tpt = self.type_tree(pair, elem_span.start_pos(), false);
            let tpe = self.u.intern_type(Type::Applied(pair, vec![self.tpd.tpe(e), self.tpd.tpe(acc)]));
            let args = self.tpd.list(&[e, acc]);
            let at = match i {
                0 => span,
                _ => Span::synthetic(elem_span.start(), self.tpd.span(acc).end()),
            };
            acc = self.tpd.node(T::AppliedTypeTree { tpt, args }, at, Some(tpe), None);
        }
        acc
    }

    /// The element types of tuple type `t` of any length (dotty's `tupleElementTypes`).
    fn tuple_element_types(&mut self, t: TypeId) -> Option<Vec<TypeId>> {
        let t = self.u.widen(t);
        self.tuple_elements_within(t, usize::MAX)
    }

    /// Whether a bind over a pattern of type `body` has expected type `pt` as its type: `body`
    /// the `TupleXXL` class and `pt` a tuple type (dotty's `typedBind`).
    pub fn binds_tuple_pt(&mut self, body: TypeId, pt: TypeId) -> bool {
        let xxl = self.resolve_full_name("scala.runtime.TupleXXL", true);
        let class = matches!(self.u.types().get(body), Type::Sym(..) | Type::TypeRef(..));
        class && xxl.is_some() && self.u.class_symbol(body) == xxl && self.tuple_element_types(pt).is_some()
    }

    /// `qual.$asInstanceOf$[to]` (dotty's `cast`).
    pub fn type_cast(&mut self, qual: TNodeId, to: TypeId) -> TNodeId {
        let tpe = self.tpd.tpe(qual);
        let any = self.u.builtins().any;
        let m = self.u.decl(any, "$asInstanceOf$").ok().and_then(|ms| ms.first().copied());
        let Some(m) = m else { return qual };
        let span = self.tpd.span(qual).to_synthetic();
        let select_tpe = self.u.intern_type(Type::Sym(Some(tpe), m));
        let name = TName { name: self.names.intern("$asInstanceOf$"), is_type: false };
        let fun = self.tpd.node(T::Select { qualifier: qual, name }, span, Some(select_tpe), Some(m));
        let tpt = self.tpd.node(T::TypeTree { inferred: false }, span.end_pos(), Some(to), None);
        let args = self.tpd.list(&[tpt]);
        self.tpd.node(T::TypeApply { fun, args }, span, Some(to), None)
    }
}
