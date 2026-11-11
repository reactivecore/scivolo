// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Which found bindings count (dotty's `qualifies` and `checkNoOuterDefs` in `findRef`): members
//! of the empty package only from inside it, inherited members only if nothing outside shadows
//! them differently.

use super::Typer;
use super::find_ref::Name;
use super::idents::Found;
use crate::ctx::{Ctx, Level, LevelId, Mode};
use syntax::ast::Tree;
use tasty::{Kind, SymbolId, Type};

impl Typer<'_> {
    /// `found` at level `l` without the members of the empty package if `ctx` is in another
    /// package; the root package stands for both; for an extractor's name, none of methods with
    /// parameters only; for an extension method's, its extension methods.
    pub fn qualifying(&mut self, mut found: Found, l: LevelId, ctx: Ctx) -> Option<Found> {
        let root = self.u.root();
        if matches!(*self.levels.get(l), Level::Members(o) if o == root)
            && self.enclosing_package(ctx.owner) != root
        {
            found.syms.retain(|&s| self.u.symbol(s).kind == Kind::Package);
        }
        if ctx.mode.is(Mode::EXTRACTOR) && found.syms.iter().all(|&s| self.is_real_method(s)) {
            return None;
        }
        self.extensions_only(found, ctx)
    }

    /// `found` without its symbols other than extension methods if `ctx` looks for those.
    pub fn extensions_only(&self, mut found: Found, ctx: Ctx) -> Option<Found> {
        if ctx.mode.is(Mode::EXTENSION_METHOD) {
            found.syms.retain(|&s| self.u.symbol(s).flags.has(tasty::tags::EXTENSION));
        }
        (!found.syms.is_empty()).then_some(found)
    }

    /// Whether `s` is a method with parameters (dotty's `isRealMethod`).
    fn is_real_method(&mut self, s: SymbolId) -> bool {
        if self.u.symbol(s).kind != Kind::Def {
            return false;
        }
        if let Some(&def) = self.syms.def_trees.get(&s) {
            return matches!(*self.ast.tree(def), Tree::DefDef { paramss, .. } if !self.ast.clause_items(paramss).is_empty());
        }
        let info = self.u.info(s);
        matches!(self.u.ty(info), Type::Method { .. } | Type::Poly { .. })
    }

    /// Whether `s` is a method whose first term parameter clause is neither `using` nor
    /// `implicit`, as `def New(tpt: Tree)(using Context)`.
    pub fn takes_explicit_args(&mut self, s: SymbolId) -> bool {
        if !self.is_real_method(s) {
            return false;
        }
        let Some(&def) = self.syms.def_trees.get(&s) else {
            let info = self.u.info(s);
            let info = self.u.poly(info).map_or(info, |p| p.result);
            return self.u.method(info).is_some_and(|m| !m.implicit);
        };
        let Tree::DefDef { paramss, .. } = *self.ast.tree(def) else { return false };
        let clauses = self.ast.clause_items(paramss);
        let first = clauses
            .iter()
            .map(|&c| self.ast.items(c).first().copied())
            .find(|p| p.is_none_or(|p| !matches!(self.ast.tree(p), Tree::TypeDef { .. })));
        let Some(first) = first else { return false };
        let modifiers = first.and_then(|p| self.ast.mods_of(p));
        !modifiers.is_some_and(|m| self.ast.modifiers(m).is(syntax::ast::flags::GIVEN_OR_IMPLICIT))
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
