// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! What an import supplies for a name in `findRef`, the alternatives of the imports next to it
//! for an extension method (dotty's `checkImportAlternatives`), and when two references agree.

use super::Typer;
use super::find_ref::{Hit, Name, Prec};
use super::idents::Found;
use crate::ctx::{Ctx, Level, LevelId};
use crate::defn::ref_type;

impl Typer<'_> {
    /// What import `i` supplies as `name` with its precedence: a named import if `allowed.0`,
    /// else the accessible members of a wildcard if `allowed.1`; in `ctx` looking for extension
    /// methods, those only.
    pub fn import_hit(
        &mut self,
        i: usize,
        name: Name,
        allowed: (bool, bool),
        ctx: Ctx,
    ) -> Option<(Found, Prec)> {
        if allowed.0
            && let Some(f) = self.named_import_ref(i, name.text, name.is_type)
        {
            let f = self.public_if_private(&f, ctx).unwrap_or(f);
            if let Some(found) = self.extensions_only(f, ctx) {
                return Some((found, Prec::NamedImport));
            }
        }
        if !allowed.1 {
            return None;
        }
        let f = self.wild_import_ref(i, name.text, name.is_type)?;
        let f = self.public_if_private(&f, ctx).unwrap_or_else(|| {
            let syms = f.syms.iter().copied().filter(|&s| self.is_accessible_from(s, f.prefix, ctx));
            Found { prefix: f.prefix, syms: syms.collect() }
        });
        Some((self.extensions_only(f, ctx)?, Prec::WildImport))
    }

    /// The accessible non-private members named as `f`'s symbols of `f`'s prefix if none of them
    /// is accessible and one is private, as `conf` of `class Foo(conf: Conf) extends Bar` with a
    /// public `conf` in `Bar` (dotty's `selection` with `excluded | Private`).
    fn public_if_private(&mut self, f: &Found, ctx: Ctx) -> Option<Found> {
        let prefix = f.prefix?;
        let private = f.syms.iter().any(|&s| self.u.symbol(s).flags.has(tasty::tags::PRIVATE));
        if !private || f.syms.iter().any(|&s| self.is_accessible_from(s, f.prefix, ctx)) {
            return None;
        }
        let is_type = self.u.is_type_symbol(f.syms[0]);
        let name = self.u.symbol(f.syms[0]).name.clone();
        let mut syms = self.non_private_members(prefix, &name);
        syms.retain(|&s| {
            self.u.is_type_symbol(s) == is_type
                && !self.u.symbol(s).flags.has(tasty::tags::GIVEN)
                && self.is_accessible_from(s, f.prefix, ctx)
        });
        Some(Found { prefix: f.prefix, syms })
    }

    /// The imports right outside import level `l`, which supplies `hit`, searched for other
    /// references of `name`: a named import replaces a wildcard `hit` and drops the alternatives
    /// found so far, references of `hit`'s precedence not the same as it or one found before go
    /// to `alts`. Returns the binding and the last import level searched.
    pub fn import_alternatives(
        &mut self,
        l: LevelId,
        mut hit: Hit,
        name: Name,
        alts: &mut Vec<Found>,
        ctx: Ctx,
    ) -> (Hit, LevelId) {
        let mut last = l;
        while let Some(o) = self.levels.outer(last)
            && let Level::Import(i) = *self.levels.get(o)
            && !self.is_root_import(i)
            && self.levels.def_scope(o) == self.levels.def_scope(l)
        {
            last = o;
            let Some((found, prec)) = self.import_hit(i, name, (true, hit.prec == Prec::WildImport), ctx)
            else {
                continue;
            };
            if prec == Prec::NamedImport && hit.prec == Prec::WildImport {
                alts.clear();
                hit = Hit { found, prec, scope: hit.scope };
            } else if !self.same_ref(&hit.found, &found) && !alts.iter().any(|a| self.same_ref(a, &found)) {
                alts.push(found);
            }
        }
        (hit, last)
    }

    /// Whether `a` and `b` refer to the same symbols from equivalent prefixes (dotty's
    /// `TypeComparer.isSameRef`).
    pub fn same_ref(&mut self, a: &Found, b: &Found) -> bool {
        if let ([s], [t]) = (&a.syms[..], &b.syms[..])
            && s != t
        {
            let (p, q) = (ref_type(self.u, a.prefix, *s), ref_type(self.u, b.prefix, *t));
            return self.u.is_subtype(p, q) && self.u.is_subtype(q, p);
        }
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
}
