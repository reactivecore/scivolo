//! Trees typed ahead of the typing that reaches them, and typed trees spliced into untyped ones.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::NodeId;

impl Typer<'_> {
    /// Records `typed` as the tree of `id` typed ahead in `ctx`.
    pub fn typed_ahead(&mut self, id: NodeId, typed: TNodeId, ctx: Ctx) {
        let entries = self.ahead.entry(id).or_default();
        entries.retain(|&(owner, _)| owner != ctx.owner);
        entries.push((ctx.owner, typed));
    }

    /// The tree typed for `id` ahead of typing it with the owner of `ctx`, or the typed tree it
    /// stands for.
    pub fn ready(&mut self, id: NodeId, ctx: Ctx) -> Option<TNodeId> {
        if let Some(entries) = self.ahead.get_mut(&id)
            && let Some(i) = entries.iter().position(|&(owner, _)| owner == ctx.owner)
        {
            let (_, t) = entries.remove(i);
            if entries.is_empty() {
                self.ahead.remove(&id);
            }
            return Some(t);
        }
        let &t = self.splices.get(&id)?;
        self.tpd.get_mut(t).span = self.ast.span(id);
        if let Some(&owner) = self.splice_owners.get(&id)
            && owner != ctx.owner
        {
            self.change_owners(t, owner, ctx.owner);
        }
        Some(t)
    }
}
