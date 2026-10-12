//! Contextual implicits that come too late: in the scope of a given definition enclosing the
//! search, at or after it (dotty's `comesTooLate`).

use super::super::Typer;
use crate::ctx::Ctx;
use tasty::tags::{GIVEN, PARAMsetter, SYNTHETIC};
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Whether contextual implicit `cand` comes too late for a search in `ctx`.
    pub fn comes_too_late(&self, cand: SymbolId, ctx: Ctx) -> bool {
        let c = self.u.symbol(cand);
        if c.kind == Kind::Param || c.flags.has(SYNTHETIC) || c.flags.has(PARAMsetter) {
            return false;
        }
        let Some(cand_start) = self.def_start(cand) else { return false };
        let mut sym = ctx.owner;
        while let Some(owner) = self.u.symbol(sym).owner {
            if Some(owner) == c.owner {
                let s = self.u.symbol(sym);
                return s.kind.is_term()
                    && s.flags.has(GIVEN)
                    && !s.name.starts_with("derived$")
                    && self.def_start(sym).is_some_and(|start| start <= cand_start);
            }
            if self.u.symbol(owner).kind == Kind::Class {
                return false;
            }
            sym = owner;
        }
        false
    }

    /// The start of the definition of `s` in the unit, if it is defined there.
    fn def_start(&self, s: SymbolId) -> Option<u32> {
        let &tree = self.syms.def_trees.get(&s)?;
        Some(self.ast.span(tree).start())
    }
}
