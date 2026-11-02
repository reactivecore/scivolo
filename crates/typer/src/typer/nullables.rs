// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Nullables.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Flow typing under explicit nulls: the references known not to be null after a tree or when a
//! condition holds, and the contexts that carry them.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use tasty::{Type, TypeId};

/// The references known not to be null after a tree completes normally (`None`: it does not),
/// and the variables it may set to null (dotty's `NotNullInfo`).
#[derive(Clone, Debug, PartialEq)]
pub struct NotNullInfo {
    asserted: Option<Vec<TypeId>>,
    retracted: Vec<TypeId>,
}

impl NotNullInfo {
    pub fn empty() -> Self {
        NotNullInfo { asserted: Some(Vec::new()), retracted: Vec::new() }
    }

    pub fn of(asserted: Vec<TypeId>) -> Self {
        NotNullInfo { asserted: Some(asserted), retracted: Vec::new() }
    }

    fn is_empty(&self) -> bool {
        *self == Self::empty()
    }

    pub fn retracted_info(self) -> Self {
        NotNullInfo { asserted: Some(Vec::new()), retracted: self.retracted }
    }

    pub fn terminated_info(self) -> Self {
        NotNullInfo { asserted: None, retracted: self.retracted }
    }

    pub fn seq(self, that: Self) -> Self {
        if self.is_empty() {
            return that;
        }
        if that.is_empty() {
            return self;
        }
        let asserted = match (self.asserted, &that.asserted) {
            (Some(a), Some(b)) => {
                Some(union(a.into_iter().filter(|r| !that.retracted.contains(r)).collect(), b))
            }
            _ => None,
        };
        NotNullInfo { asserted, retracted: union(self.retracted, &that.retracted) }
    }

    pub fn alt(self, that: Self) -> Self {
        let asserted = match (self.asserted, that.asserted) {
            (None, b) => b,
            (a, None) => a,
            (Some(a), Some(b)) => Some(a.into_iter().filter(|r| b.contains(r)).collect()),
        };
        NotNullInfo { asserted, retracted: union(self.retracted, &that.retracted) }
    }
}

pub fn union(mut a: Vec<TypeId>, b: &[TypeId]) -> Vec<TypeId> {
    a.extend(b.iter().filter(|r| !a.contains(r)).copied().collect::<Vec<_>>());
    a
}

/// The not-null infos of the contexts, each with the next older one, and the infos of trees.
#[derive(Default)]
pub struct Nullables {
    chain: Vec<(Option<u32>, NotNullInfo)>,
    pub infos: FxHashMap<TNodeId, NotNullInfo>,
}

impl Typer<'_> {
    /// `ctx` with `info` as its most recent not-null info.
    pub fn with_not_null(&mut self, ctx: Ctx, info: NotNullInfo) -> Ctx {
        if info.is_empty() {
            return ctx;
        }
        self.nullables.chain.push((ctx.nulls, info));
        Ctx { nulls: Some(self.nullables.chain.len() as u32 - 1), ..ctx }
    }

    /// `ctx` after tree `t` completed normally (dotty's `nullableContext`).
    pub fn nullable_ctx(&mut self, t: TNodeId, ctx: Ctx) -> Ctx {
        let info = self.not_null_info(t);
        self.with_not_null(ctx, info)
    }

    /// `ctx` when condition `t` is `c` (dotty's `nullableContextIf`).
    pub fn nullable_ctx_if(&mut self, t: TNodeId, c: bool, ctx: Ctx) -> Ctx {
        let info = self.not_null_info_if(t, c);
        self.with_not_null(ctx, info)
    }

    /// The context of the argument of `f` if it is `&&` or `||` (dotty's `nullableInArgContext`).
    pub fn nullable_in_arg_ctx(&mut self, f: TNodeId, ctx: Ctx) -> Ctx {
        let T::Select { qualifier, .. } = *self.tpd.tree(f) else { return ctx };
        match self.boolean_op(f).as_deref() {
            Some("&&") => self.nullable_ctx_if(qualifier, true, ctx),
            Some("||") => self.nullable_ctx_if(qualifier, false, ctx),
            _ => ctx,
        }
    }

    /// Gives local definition `stat`, if not completed yet, the not-null infos of `ctx` (dotty's
    /// `adaptCreationContext`).
    pub fn adapt_creation_ctx(&mut self, stat: syntax::ast::NodeId, ctx: Ctx) {
        if !self.u.symbol(ctx.owner).kind.is_term() {
            return;
        }
        let Some(&sym) = self.syms.by_tree.get(&stat) else { return };
        if let Some(crate::symbols::Completion::Pending { ctx: c, .. }) = self.syms.completions.get_mut(&sym)
        {
            c.nulls = ctx.nulls;
        }
    }

    fn implies_not_null(&self, ctx: Ctx, r: TypeId) -> bool {
        let mut at = ctx.nulls;
        while let Some(i) = at {
            let (outer, info) = &self.nullables.chain[i as usize];
            match &info.asserted {
                None => return true,
                Some(a) if a.contains(&r) => return true,
                _ if info.retracted.contains(&r) => return false,
                _ => at = *outer,
            }
        }
        false
    }

    /// Reference `tree`, cast to `ref.type & T` if it is known not to be null and of type
    /// `T | Null` (dotty's `toNotNullTermRef`).
    pub fn to_not_null_ref(&mut self, tree: TNodeId, ctx: Ctx) -> TNodeId {
        let r = self.tpd.tpe(tree);
        let Some(sym) = self.term_ref_sym(r) else { return tree };
        if !self.implies_not_null(ctx, r) || self.used_out_of_order(sym, ctx) {
            return tree;
        }
        if self.is_mutable_var(sym) {
            return self.unsupported_tree(self.tpd.span(tree), "flow typing of a local variable");
        }
        let stripped = self.strip_null(r);
        if stripped == r {
            return tree;
        }
        let to = self.u.intern_type(Type::And(r, stripped));
        let span = match self.tpd.span(tree) {
            s if s.point() == s.start() => s.to_synthetic(),
            s => s,
        };
        match self.cast_to(tree, to, span) {
            Ok(t) => {
                if let T::TypeApply { fun, args } = *self.tpd.tree(t) {
                    let tpt = self.tpd.items(args)[0];
                    self.tpd.get_mut(tpt).span = span.end_pos();
                    self.tpd.get_mut(fun).span = span.to_synthetic();
                }
                t
            }
            Err(what) => self.unsupported_tree(span, what),
        }
    }
}
