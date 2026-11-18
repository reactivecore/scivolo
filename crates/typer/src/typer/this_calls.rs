// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Contexts.scala
//! The context of the arguments of a self-constructor call `this(...)`: the constructor's
//! parameters, outside its class (dotty's `thisCallArgContext`).

use super::Typer;
use super::implicits::candidates::{imported_implicits, this_implicit_members};
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use names::nme;
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
            Tree::Ident { name } => self.text(name.name) == nme::CONSTRUCTOR,
            Tree::Select { qualifier, name } => {
                self.text(name.name) == nme::CONSTRUCTOR
                    && matches!(self.ast.tree(qualifier), Tree::This { .. })
            }
            _ => false,
        };
        if self_call { self.this_call_ctx(ctx) } else { ctx }
    }

    /// The context of the implicit arguments of typed application `tree` in `ctx`.
    pub fn implicit_args_ctx(&mut self, tree: TNodeId, ctx: Ctx) -> Ctx {
        let callee = self.tpd.meth_part(tree);
        let self_call = self.tpd.sym(callee).is_some_and(|s| self.u.symbol(s).name == nme::CONSTRUCTOR)
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
            secondary |= s.name == nme::CONSTRUCTOR
                && s.owner == Some(cls)
                && self.syms.primary_ctors.get(&cls) != Some(&o);
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

    /// `ctx` of a constructor body, the names of its scopes inside the class (its parameters)
    /// seen outside the class.
    fn this_call_ctx(&mut self, ctx: Ctx) -> Ctx {
        if self.u.symbol(ctx.owner).name != nme::CONSTRUCTOR {
            return ctx;
        }
        let mut level = ctx.level;
        let mut scopes = Vec::new();
        while let Some(outer) = self.levels.outer(level) {
            if let Level::Scope(entries) = self.levels.get(level) {
                scopes.push(entries.clone());
            }
            if matches!(self.levels.get(outer), Level::Members(_)) {
                let params = scopes.into_iter().rev().flatten().collect();
                let args = self.levels.push(self.levels.outer(outer), Level::Scope(params));
                self.this_call_bodies.insert(args, ctx.level);
                return Ctx { level: args, ..ctx };
            }
            level = outer;
        }
        ctx
    }

    /// Whether `ctx` is in `this(...)` arguments of a class whose body has implicits they do not
    /// see: its implicit members, inherited ones too, or those of its imports. Dotty's `Inlining`
    /// phase searches for an inline call there with them in scope.
    pub fn this_call_hides_implicits(&mut self, ctx: Ctx) -> Result<bool, &'static str> {
        let mut args = Some(ctx.level);
        let body = loop {
            let Some(l) = args else { return Ok(false) };
            if let Some(&body) = self.this_call_bodies.get(&l) {
                break body;
            }
            args = self.levels.outer(l);
        };

        let mut level = Some(body);
        while let Some(l) = level {
            match *self.levels.get(l) {
                Level::Import(i) => {
                    if !imported_implicits(self, i)?.is_empty() {
                        return Ok(true);
                    }
                }
                Level::Members(c) => return Ok(!this_implicit_members(self, c).is_empty()),
                _ => {}
            }
            level = self.levels.outer(l);
        }
        Ok(false)
    }
}
