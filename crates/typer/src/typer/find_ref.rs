//! What a simple name refers to (dotty's `findRef`): the context chain searched outward, a
//! found binding checked against what outer levels bind with lower or equal precedence.

use super::Typer;
use super::idents::Found;
use crate::ctx::{Ctx, Level, LevelId};
use tasty::Kind;

/// The precedence of a binding (dotty's `BindingPrec`), lowest first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Prec {
    PackageClause,
    WildImport,
    NamedImport,
    Definition,
}

impl Prec {
    /// Definitions beat imports, and named imports beat wildcard imports, in the same scope.
    fn beats(self, prev: Prec) -> bool {
        self == Prec::Definition || self == Prec::NamedImport && prev == Prec::WildImport
    }
}

/// A binding found so far: what, with which precedence, in which scope.
struct Hit {
    found: Found,
    prec: Prec,
    scope: Option<LevelId>,
}

/// The name looked up, as a term or a type.
#[derive(Clone, Copy)]
pub struct Name<'a> {
    pub text: &'a str,
    pub is_type: bool,
}

impl Typer<'_> {
    /// The symbols `name` refers to in `ctx`; `Err` for an ambiguous reference.
    pub fn find_ref(&mut self, name: &str, is_type: bool, ctx: Ctx) -> Result<Option<Found>, &'static str> {
        let name = Name { text: name, is_type };
        let mut prev: Option<Hit> = None;
        let mut unimported = Vec::new();
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            let scope = self.levels.def_scope(l);
            let possible = |prev: &Option<Hit>, prec: Prec| match prev {
                None => true,
                Some(h) => h.prec < prec || h.prec == prec && h.scope == scope,
            };
            match *self.levels.get(l) {
                Level::Scope(_) | Level::Members(_) => {
                    let Some(found) = self.defined(l, name).and_then(|f| self.qualifying(f, l, ctx)) else {
                        let as_type = Name { is_type: true, ..name };
                        let class = self
                            .defined(l, as_type)
                            .is_some_and(|f| f.syms.iter().any(|&s| self.u.symbol(s).kind == Kind::Class));
                        if !name.is_type && class {
                            if prev.is_some() {
                                return Err("constructor proxy and an imported term");
                            }
                            let Some(outer) = self.levels.outer(l) else { return Err("constructor proxy") };
                            return match self.find_ref(name.text, false, Ctx { level: outer, ..ctx }) {
                                Ok(Some(_)) => Err("constructor proxy shadowing a term"),
                                Ok(None) | Err("constructor proxy") => Err("constructor proxy"),
                                Err(what) => Err(what),
                            };
                        }
                        level = self.levels.outer(l);
                        continue;
                    };
                    let package = matches!(*self.levels.get(l), Level::Members(o) if self.u.symbol(o).kind == Kind::Package);
                    if !package || self.in_unit(&found) {
                        if self.is_inherited(l, &found) {
                            self.check_no_outer_defs(l, &found, name)?;
                        }
                        let hit = Hit { found, prec: Prec::Definition, scope };
                        return self.new_or_shadowed(prev, hit, l, name).map(Some);
                    }
                    if found.syms.iter().any(|&s| self.u.symbol(s).kind == Kind::Package) {
                        return Ok(Some(prev.map_or(found, |h| h.found)));
                    }
                    if prev.is_none() {
                        prev = Some(Hit { found, prec: Prec::PackageClause, scope });
                    }
                }
                Level::Import(i) => {
                    if prev.is_some() && self.is_root_import(i) {
                        return Ok(prev.map(|h| h.found));
                    }
                    if self.is_root_import(i) && self.root_site(i).is_some_and(|s| unimported.contains(&s)) {
                        level = self.levels.outer(l);
                        continue;
                    }
                    let hit = if possible(&prev, Prec::NamedImport)
                        && let Some(found) = self.named_import_ref(i, name.text, name.is_type)
                    {
                        Some(Hit { found, prec: Prec::NamedImport, scope })
                    } else if possible(&prev, Prec::WildImport) {
                        let found = self.wild_import_ref(i, name.text, name.is_type);
                        found.map(|found| Hit { found, prec: Prec::WildImport, scope })
                    } else {
                        None
                    };
                    if let Some(hit) = hit {
                        let (prec, scope) = (hit.prec, hit.scope);
                        let found = self.new_or_shadowed(prev, hit, l, name)?;
                        prev = Some(Hit { found, prec, scope });
                    } else {
                        unimported.extend(self.unimported(i));
                    }
                }
            }
            level = self.levels.outer(l);
        }
        Ok(prev.map(|h| h.found))
    }

    /// The new binding `hit` at level `l` if no inner binding `prev` conflicts with it, `prev`
    /// if it shadows it (dotty's `checkNewOrShadowed`).
    fn new_or_shadowed(
        &mut self,
        prev: Option<Hit>,
        hit: Hit,
        l: LevelId,
        name: Name,
    ) -> Result<crate::typer::idents::Found, &'static str> {
        let Some(prev) = prev else { return Ok(hit.found) };
        if self.same_ref(&prev.found, &hit.found) || prev.scope == hit.scope && hit.prec.beats(prev.prec) {
            return Ok(hit.found);
        }
        if hit.prec == Prec::WildImport && self.outer_named_import(l, name) {
            return Ok(hit.found);
        }
        Err("ambiguous reference")
    }

    /// Whether an import at `l` or outward imports `name` by name.
    fn outer_named_import(&mut self, l: LevelId, name: Name) -> bool {
        let mut level = Some(l);
        while let Some(i) = level {
            if let Level::Import(imp) = *self.levels.get(i)
                && self.named_import_ref(imp, name.text, name.is_type).is_some()
            {
                return true;
            }
            level = self.levels.outer(i);
        }
        false
    }

    fn same_ref(&mut self, a: &Found, b: &Found) -> bool {
        if a.syms != b.syms {
            return false;
        }
        let methods = a.syms.iter().all(|&s| self.u.symbol(s).kind == tasty::Kind::Def);
        match (a.prefix, b.prefix) {
            (Some(p), Some(q)) if methods && p != q => {
                a.syms.iter().all(|&s| self.u.member_info(p, s) == self.u.member_info(q, s))
            }
            (Some(p), Some(q)) => p == q || self.u.is_subtype(p, q) && self.u.is_subtype(q, p),
            (p, q) => p == q,
        }
    }

    /// Whether `found` has a symbol this unit defines.
    fn in_unit(&self, found: &Found) -> bool {
        found.syms.iter().any(|s| self.syms.flags.contains_key(s))
    }

    /// The symbols named `name` that scope or members level `l` defines.
    pub fn defined(&mut self, l: LevelId, name: Name) -> Option<Found> {
        match self.levels.get(l) {
            Level::Scope(entries) => {
                let found = entries
                    .iter()
                    .rev()
                    .find(|(n, s)| n == name.text && self.u.is_type_symbol(*s) == name.is_type);
                let sym = found?.1;
                let owner = self.u.symbol(sym).owner.filter(|&o| self.u.symbol(o).kind == Kind::Class);
                let prefix = owner.map(|o| crate::defn::owner_prefix(self.u, o));
                Some(Found { prefix, syms: vec![sym] })
            }
            &Level::Members(owner) => {
                let prefix = crate::defn::owner_prefix(self.u, owner);
                self.members_found(prefix, name.text, name.is_type)
            }
            Level::Import(_) => None,
        }
    }
}
