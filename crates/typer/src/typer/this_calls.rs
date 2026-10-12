//! The context of the arguments of a self-constructor call `this(...)`: the constructor's
//! parameters, outside its class (dotty's `thisCallArgContext`).

use super::Typer;
use crate::ctx::{Ctx, Level, Mode};
use syntax::ast::{NodeId, Tree};

impl Typer<'_> {
    /// The context of the arguments applied to `fun` in `ctx`.
    pub fn arg_ctx(&mut self, fun: NodeId, ctx: Ctx) -> Ctx {
        let mut meth = fun;
        while let Tree::Apply { fun, .. } | Tree::TypeApply { fun, .. } = *self.ast.tree(meth) {
            meth = fun;
        }
        let self_call = match *self.ast.tree(meth) {
            Tree::Ident { name } => self.text(name.name) == "<init>",
            Tree::Select { qualifier, name } => {
                self.text(name.name) == "<init>" && matches!(self.ast.tree(qualifier), Tree::This { .. })
            }
            _ => false,
        };
        if !self_call || self.u.symbol(ctx.owner).name != "<init>" {
            return ctx;
        }
        let mut level = ctx.level;
        while let Some(outer) = self.levels.outer(level) {
            if matches!(self.levels.get(outer), Level::Members(_)) {
                let params = match self.levels.get(level) {
                    Level::Scope(entries) => entries.clone(),
                    _ => Vec::new(),
                };
                let level = self.levels.push(self.levels.outer(outer), Level::Scope(params));
                return Ctx { level, mode: ctx.mode.with(Mode::THIS_CALL_ARGS), ..ctx };
            }
            level = outer;
        }
        ctx
    }
}
