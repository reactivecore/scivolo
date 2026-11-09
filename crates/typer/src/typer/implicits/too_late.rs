// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Contextual implicits that come too late: in the scope of a given definition enclosing the
//! search, at or after it (dotty's `comesTooLate`).

use super::super::Typer;
use crate::ctx::Ctx;
use tasty::tags::{GIVEN, PARAMsetter, SYNTHETIC};
use tasty::{Kind, SymbolId};

/// Whether contextual implicit `cand` comes too late for a search in `ctx`.
pub fn comes_too_late(t: &Typer<'_>, cand: SymbolId, ctx: Ctx) -> bool {
    let c = t.u.symbol(cand);
    if c.kind == Kind::Param || c.flags.has(SYNTHETIC) || c.flags.has(PARAMsetter) {
        return false;
    }
    let Some(cand_start) = def_start(t, cand) else { return false };
    let mut sym = ctx.owner;
    while let Some(owner) = t.u.symbol(sym).owner {
        if Some(owner) == c.owner {
            let s = t.u.symbol(sym);
            return s.kind.is_term()
                && s.flags.has(GIVEN)
                && !s.name.starts_with("derived$")
                && def_start(t, sym).is_some_and(|start| start <= cand_start);
        }
        if t.u.symbol(owner).kind == Kind::Class {
            return false;
        }
        sym = owner;
    }
    false
}

/// The start of the definition of `s` in the unit, if it is defined there.
fn def_start(t: &Typer<'_>, s: SymbolId) -> Option<u32> {
    let &tree = t.syms.def_trees.get(&s)?;
    Some(t.ast.span(tree).start())
}
