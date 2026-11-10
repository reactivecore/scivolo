// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Nullables.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala

mod assignments;
mod cases;
mod conditionals;
mod infos;
mod loops;
mod tracked;

use super::Typer;
use super::type_ops::TypeOps;
use crate::ctx::Ctx;
use crate::defn::Defn;
use crate::tpd::{TNodeId, Tpd};
use rustc_hash::FxHashMap;
use syntax::ast::{Ast, NodeId};
use tasty::{SymbolId, TypeId};

/// Holds the references known not to be null after a tree completes normally (`None`: it does
/// not), and the variables it may set to null (dotty's `NotNullInfo`): after
/// `if x == null then return`, `x` is known not null; after `x = null`, a `var x` no longer is.
#[derive(Clone, Debug, PartialEq)]
pub struct NotNullInfo {
    asserted: Option<Vec<TypeId>>,
    retracted: Vec<TypeId>,
}

impl NotNullInfo {
    /// Makes the info of a tree that knows nothing.
    pub fn empty() -> Self {
        NotNullInfo { asserted: Some(Vec::new()), retracted: Vec::new() }
    }

    /// Makes the info that knows the references `asserted` not null.
    pub fn of(asserted: Vec<TypeId>) -> Self {
        NotNullInfo { asserted: Some(asserted), retracted: Vec::new() }
    }

    /// Makes the info that drops what is known of the variables `retracted`.
    pub fn retracting(retracted: Vec<TypeId>) -> Self {
        NotNullInfo { asserted: Some(Vec::new()), retracted }
    }

    /// The references known not to be null.
    pub fn asserted(&self) -> &[TypeId] {
        self.asserted.as_deref().unwrap_or_default()
    }

    fn is_empty(&self) -> bool {
        *self == Self::empty()
    }

    /// Keeps only what the tree retracts, as for a loop body that may run any number of times.
    pub fn retracted_info(self) -> Self {
        NotNullInfo { asserted: Some(Vec::new()), retracted: self.retracted }
    }

    /// Marks the tree as never completing normally, as a `return` or a call of type `Nothing`.
    pub fn terminated_info(self) -> Self {
        NotNullInfo { asserted: None, retracted: self.retracted }
    }

    /// Combines the info of a tree with that of the tree evaluated after it.
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

    /// Combines the infos of two alternative paths, as the branches of an `if`.
    pub fn alt(self, that: Self) -> Self {
        let asserted = match (self.asserted, that.asserted) {
            (None, b) => b,
            (a, None) => a,
            (Some(a), Some(b)) => Some(a.into_iter().filter(|r| b.contains(r)).collect()),
        };
        NotNullInfo { asserted, retracted: union(self.retracted, &that.retracted) }
    }
}

/// Appends the references of `b` that `a` lacks to `a`.
pub fn union(mut a: Vec<TypeId>, b: &[TypeId]) -> Vec<TypeId> {
    a.extend(b.iter().filter(|r| !a.contains(r)).copied().collect::<Vec<_>>());
    a
}

/// Holds the not-null facts of the flow typing under explicit nulls: in
/// `if x != null then x.length else 0` the `then` branch knows `x` is not null, and so does the
/// code after `if x == null then return` (dotty's `NotNullInfo`s in the `Context`). One field of
/// `Typer`; a context refers to its most recent facts by index (ADR 0114).
#[derive(Default)]
pub struct Nullables {
    /// The not-null info of each context with the index of the next older one.
    chain: Vec<(Option<u32>, NotNullInfo)>,
    /// The info of each typed tree computed so far.
    infos: FxHashMap<TNodeId, NotNullInfo>,
    /// The local variables flow typing tracks, found on first use.
    assignments: Option<assignments::Assignments>,
}

/// Lends the flow typing what it reads: the typed trees, the types with the constraint, the
/// definitions, and the unit's syntax trees with the definition tree of each symbol.
pub struct Flow<'a> {
    tpd: &'a Tpd,
    types: TypeOps<'a>,
    defn: &'a Defn,
    ast: &'a Ast,
    unit: NodeId,
    def_trees: &'a FxHashMap<SymbolId, NodeId>,
}

/// Splits the typer into its not-null facts and what the flow typing reads.
fn flow<'a>(t: &'a mut Typer<'_>) -> (&'a mut Nullables, Flow<'a>) {
    let flow = Flow {
        tpd: &t.tpd,
        types: TypeOps { u: t.u, constraint: &mut t.constraint },
        defn: &t.defn,
        ast: t.ast,
        unit: t.unit,
        def_trees: &t.syms.def_trees,
    };
    (&mut t.nullables, flow)
}

impl Nullables {
    /// Gives `ctx` the not-null info `info` as its most recent one.
    pub fn with_not_null(&mut self, ctx: Ctx, info: NotNullInfo) -> Ctx {
        if info.is_empty() {
            return ctx;
        }
        self.chain.push((ctx.nulls, info));
        Ctx { nulls: Some(self.chain.len() as u32 - 1), ..ctx }
    }

    /// Lists the not-null infos of `ctx`, the most recent first.
    fn not_null_infos(&self, ctx: Ctx) -> impl Iterator<Item = &NotNullInfo> {
        std::iter::successors(ctx.nulls, |&i| self.chain[i as usize].0).map(|i| &self.chain[i as usize].1)
    }

    /// Tells whether `ctx` knows reference `r` not to be null: a more recent info asserts it, or
    /// says the code is not reached, before one retracts it.
    fn implies_not_null(&self, ctx: Ctx, r: TypeId) -> bool {
        let mut at = ctx.nulls;
        while let Some(i) = at {
            let (outer, info) = &self.chain[i as usize];
            match &info.asserted {
                None => return true,
                Some(a) if a.contains(&r) => return true,
                _ if info.retracted.contains(&r) => return false,
                _ => at = *outer,
            }
        }
        false
    }
}

impl Typer<'_> {
    /// Finds the context of definition `stat` in a statement sequence whose not-null infos were
    /// `initial`: `ctx` if `stat` is local and not completed yet, which then gets the infos of
    /// `ctx` (dotty's `adaptCreationContext`), else `ctx` with the `initial` infos.
    pub fn definition_ctx(&mut self, stat: NodeId, initial: Option<u32>, ctx: Ctx) -> Ctx {
        let sym = self.syms.by_tree.get(&stat).copied();
        let pending = match sym.and_then(|s| self.syms.completions.get_mut(&s)) {
            Some(crate::symbols::Completion::Pending { ctx: c, .. }) => Some(c),
            _ => None,
        };
        match pending {
            Some(c) if self.u.symbol(ctx.owner).kind.is_term() => {
                c.nulls = ctx.nulls;
                ctx
            }
            _ => Ctx { nulls: initial, ..ctx },
        }
    }
}
