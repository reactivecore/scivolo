// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Which found bindings count (dotty's `qualifies` and `checkNoOuterDefs` in `findRef`): members
//! of the empty package only from inside it, inherited members only if nothing outside shadows
//! them differently.

use super::Typer;
use super::find_ref::Name;
use super::idents::Found;
use crate::ctx::{Ctx, Level, LevelId, Mode};
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// `found` at level `l` without the members of the empty package if `ctx` is in another
    /// package; the root package stands for both; without methods for an extractor's name.
    pub fn qualifying(&self, mut found: Found, l: LevelId, ctx: Ctx) -> Option<Found> {
        let root = self.u.root();
        if matches!(*self.levels.get(l), Level::Members(o) if o == root)
            && self.enclosing_package(ctx.owner) != root
        {
            found.syms.retain(|&s| self.u.symbol(s).kind == Kind::Package);
        }
        if ctx.mode.is(Mode::EXTRACTOR) {
            found.syms.retain(|&s| self.u.symbol(s).kind != Kind::Def);
        }
        (!found.syms.is_empty()).then_some(found)
    }

    fn enclosing_package(&self, owner: SymbolId) -> SymbolId {
        let mut o = owner;
        while self.u.symbol(o).kind != Kind::Package {
            match self.u.symbol(o).owner {
                Some(p) => o = p,
                None => break,
            }
        }
        o
    }

    /// Whether `found` at class members level `l` refers only to members the class inherits.
    pub fn is_inherited(&self, l: LevelId, found: &Found) -> bool {
        let Level::Members(cls) = *self.levels.get(l) else { return false };
        self.u.symbol(cls).kind == Kind::Class
            && found.syms.iter().all(|&s| self.u.symbol(s).owner != Some(cls))
    }

    /// Checks that no scope or class outside level `l`, up to the first package, defines `name`
    /// differently from the inherited `found`.
    pub fn check_no_outer_defs(&mut self, l: LevelId, found: &Found, name: Name) -> Result<(), &'static str> {
        let mut level = self.levels.outer(l);
        while let Some(o) = level {
            if matches!(*self.levels.get(o), Level::Members(p) if self.u.symbol(p).kind == Kind::Package) {
                return Ok(());
            }
            let cls = match *self.levels.get(o) {
                Level::Members(c) if self.u.symbol(c).kind == Kind::Class => Some(c),
                _ => None,
            };
            if let Some(outer) = self.defined(o, name)
                && outer.syms.iter().any(|&s| {
                    !found.syms.contains(&s) && cls.is_none_or(|c| self.u.symbol(s).owner == Some(c))
                })
            {
                return Err("ambiguous reference: inherited and defined outside");
            }
            level = self.levels.outer(o);
        }
        Ok(())
    }
}
