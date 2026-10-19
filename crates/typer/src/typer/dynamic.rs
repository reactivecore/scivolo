//! Selections over `scala.Dynamic` (dotty's `Dynamic`): `qual.name` typed as a call of
//! `selectDynamic`, `applyDynamic`, `applyDynamicNamed` or `updateDynamic` with the name.

use super::implicits::conversions::Goal;
use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::constant::Constant;
use syntax::ast::{List, NodeId, Span, TName, Tree};
use syntax::config::Features;

/// A selection over `Dynamic` left for the application or assignment around it (dotty's
/// `TryDynamicCallType`).
pub struct DynamicCall {
    qual: TNodeId,
    name: String,
    span: Span,
    targs: Vec<NodeId>,
}

fn is_dynamic_method(name: &str) -> bool {
    matches!(name, "applyDynamic" | "selectDynamic" | "updateDynamic" | "applyDynamicNamed")
}

impl Typer<'_> {
    /// Selection `id` of term `name`, which `qual` lacks, as a dynamic call if `qual` derives
    /// from `Dynamic` (dotty's `tryDynamic`): a pending call for an application or assignment,
    /// `qual.selectDynamic("name")` otherwise.
    pub fn try_dynamic(
        &mut self,
        id: NodeId,
        qual: TNodeId,
        name: TName,
        pt: Pt,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        let text = self.text(name.name).to_owned();
        if name.is_type || is_dynamic_method(&text) || !self.derives_dynamic(qual) {
            return None;
        }
        if ctx.mode.is(Mode::PATTERN) {
            return Some(self.unsupported(id, "Dynamic member in a pattern"));
        }
        let call = DynamicCall { qual, name: text, span: self.ast.span(id), targs: Vec::new() };
        if matches!(pt, Pt::Fun(_) | Pt::PolyFun) || self.dynamic_lhs == Some(id) {
            return Some(self.pending_dynamic(call));
        }
        Some(self.typed_dynamic_select(call, pt, ctx))
    }

    /// `try_dynamic` for member `name` of `qual` that is inaccessible, unless an extension method
    /// or an implicit conversion may supply it.
    pub fn try_dynamic_inaccessible(
        &mut self,
        id: NodeId,
        qual: TNodeId,
        name: TName,
        pt: Pt,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        let text = self.text(name.name).to_owned();
        match self.supplied_otherwise(qual, &text, ctx) {
            Ok(false) => self.try_dynamic(id, qual, name, pt, ctx),
            _ => None,
        }
    }

    /// `f.apply` as a pending dynamic call when `f`, applied to arguments, derives from `Dynamic`
    /// and is not itself a dynamic call; an error where an extension method or an implicit
    /// conversion may supply `apply` (dotty's `tryExt` before `tryDynamic`).
    pub fn dynamic_apply_insertion(&mut self, f: TNodeId, ctx: Ctx) -> Result<Option<TNodeId>, &'static str> {
        let callee = self.tpd.sym(self.tpd.meth_part(f));
        if callee.is_some_and(|m| is_dynamic_method(&self.u.symbol(m).name)) || !self.derives_dynamic(f) {
            return Ok(None);
        }
        if self.supplied_otherwise(f, "apply", ctx)? {
            return Err("apply of a Dynamic value an extension or conversion may supply");
        }
        let call = DynamicCall { qual: f, name: "apply".into(), span: self.tpd.span(f), targs: Vec::new() };
        Ok(Some(self.pending_dynamic(call)))
    }

    /// Whether a visible extension method or an implicit conversion may supply member `name` of
    /// `qual`.
    fn supplied_otherwise(&mut self, qual: TNodeId, name: &str, ctx: Ctx) -> Result<bool, &'static str> {
        if self.extension_visible(name, ctx) {
            return Ok(true);
        }
        let before = self.constraint.attempt();
        let viewed = self.viewed(qual, Goal::Member(name), ctx);
        self.constraint.abandon(before);
        Ok(viewed?.is_some())
    }

    /// Refuses class template `rhs` with `parents` extending `scala.Dynamic` where `ctx` does not
    /// enable `scala.language.dynamics`.
    pub fn check_dynamics_enabled(&mut self, rhs: NodeId, parents: &[tasty::TypeId], ctx: Ctx) {
        let dynamic = parents
            .iter()
            .any(|&p| self.u.class_symbol(p).is_some_and(|c| self.u.full_name(c) == "scala.Dynamic"));
        if dynamic && !self.feature_enabled(Features::DYNAMICS, "dynamics", ctx) {
            self.unsupported(rhs, "extension of Dynamic without the dynamics feature");
        }
    }

    /// Whether the type of `qual` derives from `scala.Dynamic`.
    pub fn derives_dynamic(&mut self, qual: TNodeId) -> bool {
        let tpe = self.tpd.tpe(qual);
        let tpe = self.zonk(tpe);
        self.derives_from_named(tpe, "scala.Dynamic")
    }

    /// A placeholder tree of the error type standing for `call`.
    fn pending_dynamic(&mut self, call: DynamicCall) -> TNodeId {
        let name = TName { name: self.name(&call.name), is_type: false };
        let tpe = self.u.intern_type(tasty::Type::Error);
        let tree = self.tpd.node(T::Select { qualifier: call.qual, name }, call.span, Some(tpe), None);
        self.dynamic_calls.insert(tree, call);
        tree
    }

    /// Whether `f` is a pending dynamic call.
    pub fn is_pending_dynamic(&self, f: TNodeId) -> bool {
        self.dynamic_calls.contains_key(&f)
    }

    /// Refuses the pending dynamic calls no application or assignment took, at the smallest.
    pub fn check_dynamic_calls(&mut self) {
        let smallest =
            self.dynamic_calls.values().map(|c| c.span).min_by_key(|s| (s.end() - s.start(), s.start()));
        if let Some(span) = smallest {
            self.unsupported_at(span, "Dynamic member applied through another tree");
        }
    }

    /// Pending call `f` applied to type arguments `targs`: still pending under an application,
    /// `qual.selectDynamic[targs]("name")` otherwise.
    pub fn dynamic_type_apply(&mut self, f: TNodeId, targs: Vec<NodeId>, pt: Pt, ctx: Ctx) -> TNodeId {
        let mut call = self.dynamic_calls.remove(&f).expect("pending dynamic call");
        call.targs = targs;
        if matches!(pt, Pt::Fun(_)) {
            self.dynamic_calls.insert(f, call);
            return f;
        }
        self.typed_dynamic_select(call, pt, ctx)
    }

    fn typed_dynamic_select(&mut self, call: DynamicCall, pt: Pt, ctx: Ctx) -> TNodeId {
        let (fun, name) = self.dynamic_fun(&call, "selectDynamic");
        let typed = self.typed_dynamic_call(fun, &[name], pt, ctx);
        if call.targs.is_empty() {
            self.tpd.get_mut(typed).span = call.span;
        }
        typed
    }

    /// Application `id` of pending call `f` to the arguments of `proto` (dotty's
    /// `typedDynamicApply`): `qual.applyDynamic("name")(args)`, or with named arguments
    /// `qual.applyDynamicNamed("name")(("x", a), ("", b))`.
    pub fn dynamic_apply(&mut self, id: NodeId, f: TNodeId, proto: usize, pt: Pt, ctx: Ctx) -> TNodeId {
        let call = self.dynamic_calls.remove(&f).expect("pending dynamic call");
        let args = self.protos[proto].args.clone();
        let named = args.iter().any(|&a| matches!(self.ast.tree(a), Tree::NamedArg { .. }));
        if !named {
            let fun = self.core_dynamic(&call, "applyDynamic");
            return self.typed_dynamic_call(fun, &args, pt, ctx);
        }
        let mut tuples = Vec::new();
        for a in args {
            let (name, arg) = match *self.ast.tree(a) {
                Tree::NamedArg { name, arg } => (self.text(name.name).to_owned(), arg),
                _ if self.is_sequence_arg(a) => {
                    return self.unsupported(id, "applyDynamicNamed with a vararg argument");
                }
                _ => (String::new(), a),
            };
            let span = self.ast.span(arg);
            let label = self.string_literal(&name, span.start_pos());
            let tuple2 = TName { name: self.name("scala.Tuple2"), is_type: false };
            let fun = self.ast.node(Tree::TypedSplice { sym: tuple2 });
            self.ast.with_span(fun, span.start_pos());
            let elems = self.ast.list(&[label, arg]);
            let tuple = self.ast.node(Tree::Apply { fun, args: elems });
            self.ast.with_span(tuple, span.to_synthetic());
            tuples.push(tuple);
        }
        let fun = self.core_dynamic(&call, "applyDynamicNamed");
        self.typed_dynamic_call(fun, &tuples, pt, ctx)
    }

    /// Pending call `lhs` assigned `rhs`, typed for `pt` (dotty's `typedDynamicAssign`):
    /// `qual.updateDynamic("name")(rhs)`.
    pub fn typed_dynamic_assign(&mut self, lhs: TNodeId, rhs: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        let call = self.dynamic_calls.remove(&lhs).expect("pending dynamic call");
        let fun = self.core_dynamic(&call, "updateDynamic");
        self.typed_dynamic_call(fun, &[rhs], pt, ctx)
    }

    /// `fun(args)` typed for `pt`, spanning both.
    fn typed_dynamic_call(&mut self, fun: NodeId, args: &[NodeId], pt: Pt, ctx: Ctx) -> TNodeId {
        let (apply, args) = self.applied(fun, args);
        self.typed_apply(apply, fun, args, pt, ctx)
    }

    /// The untyped `fun(args)`, spanning both, and its argument list.
    pub fn applied(&mut self, fun: NodeId, args: &[NodeId]) -> (NodeId, List) {
        let span = self.envelope(self.ast.span(fun), args);
        let args = self.ast.list(args);
        let apply = self.ast.node(Tree::Apply { fun, args });
        self.ast.with_span(apply, span);
        (apply, args)
    }

    /// `qual.dyn_name[targs]("name")` for `call` (dotty's `coreDynamic`).
    fn core_dynamic(&mut self, call: &DynamicCall, dyn_name: &str) -> NodeId {
        let (fun, name) = self.dynamic_fun(call, dyn_name);
        self.applied(fun, &[name]).0
    }

    /// `qual.dyn_name[targs]` for `call` and the literal `"name"` it is applied to.
    fn dynamic_fun(&mut self, call: &DynamicCall, dyn_name: &str) -> (NodeId, NodeId) {
        let qualifier = self.spliced(call.qual);
        let name = TName { name: self.name(dyn_name), is_type: false };
        let mut fun = self.ast.node(Tree::Select { qualifier, name });
        self.ast.with_span(fun, call.span);
        if !call.targs.is_empty() {
            let span = self.envelope(call.span, &call.targs);
            let args = self.ast.list(&call.targs);
            fun = self.ast.node(Tree::TypeApply { fun, args });
            self.ast.with_span(fun, span);
        }
        let end = self.ast.span(fun).end_pos();
        (fun, self.string_literal(&call.name, end))
    }

    /// `first` widened to the spans of `rest`, synthetic (dotty's `envelope`).
    fn envelope(&self, first: Span, rest: &[NodeId]) -> Span {
        rest.iter().fold(first, |span, &t| span.union(self.ast.span(t))).to_synthetic()
    }

    pub fn string_literal(&mut self, s: &str, span: Span) -> NodeId {
        let value = Constant::String(s.encode_utf16().collect());
        let literal = self.ast.node(Tree::Literal { value });
        self.ast.with_span(literal, span);
        literal
    }
}
