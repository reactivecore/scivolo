// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Named tuples `(a: A, b: B)` as `NamedTuple.NamedTuple[("a", "b"), (A, B)]`: types, literals
//! cast from their values tuple, and element selection `x.a` as `NamedTuple.apply(x)(i)`.

use super::small_tuples::MAX_TUPLE_ARITY;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::constant::Constant;
use syntax::ast::{List, NodeId, Span, TName, Tree};
use tasty::TypeId;

impl Typer<'_> {
    /// The names and values of tuple `trees` if any element is a named argument; `Err` if only
    /// some are or a name repeats.
    pub fn named_elements(&self, trees: List) -> Option<Result<Vec<(String, NodeId)>, &'static str>> {
        let elems = self.ast.items(trees);
        let named: Vec<(String, NodeId)> = elems
            .iter()
            .filter_map(|&e| match *self.ast.tree(e) {
                Tree::NamedArg { name, arg } => Some((self.text(name.name).to_owned(), arg)),
                _ => None,
            })
            .collect();
        match named.len() {
            0 => None,
            n if n < elems.len() => Some(Err("tuple of named and unnamed elements")),
            _ if named.iter().enumerate().any(|(i, (n, _))| named[..i].iter().any(|(m, _)| m == n)) => {
                Some(Err("duplicate tuple element name"))
            }
            _ => Some(Ok(named)),
        }
    }

    /// Whether pattern `id` is a tuple with named elements and `pt` a named tuple (dotty's
    /// `isNamedTuplePattern`).
    pub fn is_named_tuple_pattern(&mut self, id: NodeId, pt: TypeId) -> bool {
        let Tree::Tuple { trees } = *self.ast.tree(id) else { return false };
        self.named_elements(trees).is_some() && self.is_named_tuple_type(pt)
    }

    /// Named tuple type `id` (dotty's `desugar.tuple` of a type).
    pub fn typed_named_tuple_type(&mut self, id: NodeId, elems: Vec<(String, NodeId)>, ctx: Ctx) -> TNodeId {
        let names: Vec<String> = elems.iter().map(|(n, _)| n.clone()).collect();
        let trees: Vec<TNodeId> = elems.iter().map(|&(_, e)| self.typed_type(e, ctx)).collect();
        let values: Vec<TypeId> = trees.iter().map(|&t| self.tpd.tpe(t)).collect();
        match self.named_tuple_type(&names, &values) {
            Some(t) => self.named_tuple_tree(t, &names, &trees, self.ast.span(id)),
            None => self.unsupported(id, "named tuple classes missing from the library"),
        }
    }

    /// Named tuple literal `id`: `TupleN(…)`, above 22 elements the tuple without names, typed
    /// against `pt` stripped of its names, cast to the named tuple of its element types (dotty's
    /// `NamedTuple.build` after inlining).
    pub fn typed_named_tuple(
        &mut self,
        id: NodeId,
        elems: Vec<(String, NodeId)>,
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let names: Vec<String> = elems.iter().map(|(n, _)| n.clone()).collect();
        let args: Vec<NodeId> = elems.iter().map(|&(_, e)| e).collect();
        let span = self.ast.span(id);
        let values = if args.len() > MAX_TUPLE_ARITY {
            let trees = self.ast.list(&args);
            let values = self.ast.node(Tree::Tuple { trees });
            self.ast.with_span(values, span);
            values
        } else {
            self.tuple_apply(&args, self.ast.span(args[0]).start_pos(), span)
        };
        let named_pt = match pt {
            Pt::Type(t) => self.named_tuple_parts(t).filter(|(n, _)| *n == names).map(|(_, v)| (t, v)),
            _ => None,
        };
        let values_pt = named_pt.as_ref().and_then(|(_, v)| self.values_type(v));
        let typed = self.typed(values, values_pt.map_or(Pt::Any, Pt::Type), ctx);
        if args.len() <= MAX_TUPLE_ARITY
            && !self.is_error(typed)
            && let Some(built) = self.named_tuple_build(&names, typed, span, ctx)
        {
            return built;
        }
        if let Some((t, _)) = named_pt {
            return self.type_cast(typed, t);
        }
        let tpe = self.tpd.tpe(typed);
        let widened = self.u.widen(tpe);
        let Some(values) = self.elements_of(widened) else {
            return self.unsupported(id, "named tuple of an untupled value");
        };
        match self.named_tuple_type(&names, &values) {
            Some(to) => self.type_cast(typed, to),
            None => self.unsupported(id, "named tuple classes missing from the library"),
        }
    }

    /// `scala.TupleN(args…)` at `span`, its function at `fun_at`.
    pub fn tuple_apply(&mut self, args: &[NodeId], fun_at: Span, span: Span) -> NodeId {
        let module = TName { name: self.name(&format!("scala.Tuple{}", args.len())), is_type: false };
        let fun = self.ast.node(Tree::TypedSplice { sym: module });
        self.ast.with_span(fun, fun_at);
        let args = self.ast.list(args);
        let app = self.ast.node(Tree::Apply { fun, args });
        self.ast.with_span(app, span);
        app
    }

    /// `qual.name` as `NamedTuple.apply(qual)(i)` if `qual` is a named tuple with element `name`
    /// (dotty's `tryNamedTupleSelection`).
    pub fn named_tuple_select(
        &mut self,
        id: NodeId,
        qual: TNodeId,
        name: TName,
        pt: Pt,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        if name.is_type {
            return None;
        }
        let tpe = self.tpd.tpe(qual);
        let widened = self.u.widen(tpe);
        let (names, values) = self.derived_named_tuple_parts(widened)?;
        let i = names.iter().position(|n| n == self.text(name.name))?;
        if names.iter().any(|n| n == "apply") {
            return Some(
                self.error(id, "Named tuples that define an `apply` field do not allow field selection."),
            );
        }
        (i < values.len()).then(|| self.indexed_element(id, qual, i, pt, ctx))
    }

    /// Selection `id` of element `i` of named tuple `qual` as `NamedTuple.apply(qual)(i)`.
    fn indexed_element(&mut self, id: NodeId, qual: TNodeId, i: usize, pt: Pt, ctx: Ctx) -> TNodeId {
        let span = self.ast.span(id);
        let module = TName { name: self.name("scala.NamedTuple"), is_type: false };
        let qualifier = self.ast.node(Tree::TypedSplice { sym: module });
        let name = TName { name: self.name("apply"), is_type: false };
        let fun = self.ast.node(Tree::Select { qualifier, name });
        let tuple = self.spliced(qual);
        let args = self.ast.list(&[tuple]);
        let extension = self.ast.node(Tree::Apply { fun, args });
        let index = self.ast.node(Tree::Literal { value: Constant::Int(i as i32) });
        let args = self.ast.list(&[index]);
        let apply = self.ast.node(Tree::Apply { fun: extension, args });
        let qual_span = self.tpd.span(qual);
        self.ast.with_span(qualifier, qual_span.start_pos().to_synthetic());
        self.ast.with_span(fun, qual_span);
        self.ast.with_span(extension, qual_span.to_synthetic());
        self.ast.with_span(index, qual_span.end_pos().to_synthetic());
        self.ast.with_span(apply, span);
        let typed = self.typed(apply, pt, ctx);
        let tpe = self.tpd.tpe(typed);
        let tpe = match self.fully_defined(tpe) {
            Ok(t) => t,
            Err(what) => return self.unsupported(id, what),
        };
        self.tpd.get_mut(typed).tpe = Some(self.u.normalized(tpe));
        typed
    }
}
