// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Capture sets on function and by-name types, `A ->{c} B` and `->{c} A` (dotty's
//! `typedFunctionType` and `typedByNameTypeTree` of a `CapturesAndResult`).

use super::seen_from::member_of;
use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use names::nme;
use syntax::ast::{List, NodeId, Tree};
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// Function type `id` of result `result` with capture set `refs` as
    /// `(args) -> result @retains[refs]`.
    pub fn typed_retaining_function(
        &mut self,
        id: NodeId,
        (refs, result): (List, NodeId),
        ctx: Ctx,
    ) -> TNodeId {
        let Tree::Function { args, mods, wildcard, .. } = self.ast.tree(id).clone() else {
            unreachable!("a function type")
        };
        let function = self.ast.node(Tree::Function { args, body: result, mods, wildcard });
        let span = self.ast.span(id);
        self.ast.with_span(function, span);
        let refs = self.ast.items(refs).to_vec();
        let retaining = self.ast.make_retaining(self.names, function, &refs, nme::RETAINS);
        self.typed_type(retaining, ctx)
    }

    /// Records for inferred type tree `tpt` of the `i`th parameter of a function value, in a
    /// capture-checked file, the `i`th argument of expected function type `pt` if it has
    /// capture-set annotations: `Label^{}` of `l` in `val f: (Label^{}) => Int = l => 1`.
    pub fn record_formal_captures(&mut self, tpt: TNodeId, pt: Pt, i: usize) {
        let Pt::Type(pt) = pt else { return };
        if !self.features.capture_checking() || !matches!(self.tpd.tree(tpt), T::TypeTree { inferred: true })
        {
            return;
        }
        let Some((tycon, args)) = self.u.applied(pt) else { return };
        let Some(&formal) = args.get(i).filter(|_| i + 1 < args.len()) else { return };
        let function =
            |n: &str| n.trim_start_matches("Impure").trim_start_matches("Context").starts_with("Function");
        let is_function = self.u.sym_of(tycon).is_some_and(|s| member_of(self.u, s, "scala", function));
        if is_function && has_captures(self.u, formal) {
            self.tpd.set_captured_type(tpt, formal);
        }
    }

    /// By-name type `id` of result `result`.
    pub fn typed_by_name_type(&mut self, id: NodeId, result: NodeId, ctx: Ctx) -> TNodeId {
        let result = self.by_name_retaining(result);
        let result = self.typed_type(result, ctx);
        let tpe = self.u.intern_type(Type::ByName(self.tpd.tpe(result)));
        self.tpd.node(T::ByNameTypeTree { result }, self.ast.span(id), Some(tpe), None)
    }

    /// By-name result `result`, `parent @retainsByName[refs]` if it has capture set `refs`.
    fn by_name_retaining(&mut self, result: NodeId) -> NodeId {
        let Tree::CapturesAndResult { refs, parent } = *self.ast.tree(result) else { return result };
        let refs = self.ast.items(refs).to_vec();
        self.ast.make_retaining(self.names, parent, &refs, nme::RETAINS_BY_NAME)
    }
}

/// Determines whether type `t` has a capture-checking annotation in it.
pub fn has_captures(u: &mut tasty::Universe, t: TypeId) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        if let Some((_, a)) = u.annotated(part) {
            found |= is_retains_like(u, a);
        }
        None
    });
    found
}

/// Whether annotation class type `a` is capture checking's `retains`, `retainsByName` or
/// `retainsCap` (dotty's `isRetainsLike`).
pub fn is_retains_like(u: &mut tasty::Universe, a: TypeId) -> bool {
    u.class_symbol(a).is_some_and(|c| member_of(u, c, "scala.annotation", |n| n.starts_with(nme::RETAINS)))
}
