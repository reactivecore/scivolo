//! Applications of a value without `apply` (dotty's `MethodDoesNotTakeParameters` from
//! `adaptToArgs`), reported where no extension method or implicit view may supply one.

use super::Typer;
use super::implicits::conversions::Goal;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::flags as f;
use syntax::ast::{NodeId, Tree};
use tasty::Kind;
use tasty::tags::{EXTENSION, LAZY, MUTABLE, OBJECT};

impl Typer<'_> {
    /// `MethodDoesNotTakeParameters` for the application of reference `fun`, typed as `f`, to
    /// `n` arguments; `None` where dotty may still apply it or word it otherwise.
    pub fn does_not_take_parameters(
        &mut self,
        fun: NodeId,
        f: TNodeId,
        n: usize,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        if n == 0 || !matches!(self.ast.tree(fun), Tree::Ident { .. } | Tree::Select { .. }) {
            return None;
        }
        let s = self.tpd.sym(f)?;
        let source = self.syms.flags.get(&s).copied().unwrap_or_default();
        let sym = self.u.symbol(s);
        let plain = !sym.flags.has(OBJECT) && !sym.flags.has(LAZY) && !sym.flags.has(MUTABLE);
        let kind = match sym.kind {
            Kind::Val if plain && !source.is(f::LAZY) && !source.is(f::MUTABLE) => "value",
            Kind::Param => "parameter",
            _ => return None,
        };
        let (name, owner) = (sym.name.clone(), sym.owner?);
        let (q, site) = self.member_site(self.tpd.tpe(f))?;
        let goal = Goal::Member("apply");
        let viewed = self.script_may_supply(None)
            || self.extension_visible("apply", ctx)
            || self
                .implicit_scope(site)
                .map_or(true, |s| s.into_iter().any(|c| self.may_view(c.sym, q, goal)));
        let mut objects = vec![self.resolve_full_name("scala.Predef$", true)?];
        if viewed || name.contains('$') || self.imported_objects(ctx, &mut objects).is_none() {
            return None;
        }
        for o in objects {
            for base in self.u.linearization(o) {
                for d in self.u.decls(base).ok()? {
                    if !self.u.symbol(d).flags.has(EXTENSION) && self.may_view(d, q, goal) {
                        return None;
                    }
                }
            }
        }
        let location = match self.u.symbol(owner).kind {
            Kind::Def | Kind::Val => String::new(),
            _ if kind == "parameter" => return None,
            _ => self.location(owner)?,
        };
        Some(self.error(fun, super::errors::does_not_take_parameters(&format!("{kind} {name}{location}"))))
    }
}
