//! The context of the arguments of a self-constructor call `this(...)`: the constructor's
//! parameters, outside its class (dotty's `thisCallArgContext`).

use super::Typer;
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{NodeId, Tree};
use tasty::SymbolId;

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
        if self_call { self.this_call_ctx(ctx) } else { ctx }
    }

    /// The context of the implicit arguments of typed application `tree` in `ctx`.
    pub fn implicit_args_ctx(&mut self, tree: TNodeId, ctx: Ctx) -> Ctx {
        let callee = self.tpd.meth_part(tree);
        let self_call = self.tpd.sym(callee).is_some_and(|s| self.u.symbol(s).name == "<init>")
            && matches!(*self.tpd.tree(callee), T::Select { qualifier, .. }
                if matches!(self.tpd.tree(qualifier), T::This { .. }));
        if self_call { self.this_call_ctx(ctx) } else { ctx }
    }

    /// Whether `ctx` is inside the arguments of a self-constructor call of class `cls`: a
    /// secondary constructor of `cls` owns it and the members of `cls` are not in scope.
    pub fn in_this_call_args(&self, cls: SymbolId, ctx: Ctx) -> bool {
        let mut owner = Some(ctx.owner);
        let mut secondary = false;
        while let Some(o) = owner.filter(|&o| o != cls) {
            let s = self.u.symbol(o);
            secondary |=
                s.name == "<init>" && s.owner == Some(cls) && self.syms.primary_ctors.get(&cls) != Some(&o);
            owner = s.owner;
        }
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            if matches!(self.levels.get(l), Level::Members(c) if *c == cls) {
                return false;
            }
            level = self.levels.outer(l);
        }
        secondary
    }

    /// `ctx` of a constructor body, its parameters seen outside the class.
    fn this_call_ctx(&mut self, ctx: Ctx) -> Ctx {
        if self.u.symbol(ctx.owner).name != "<init>" {
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
                return Ctx { level, ..ctx };
            }
            level = outer;
        }
        ctx
    }
}
