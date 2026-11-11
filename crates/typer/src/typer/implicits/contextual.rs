// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! The implicits visible in a context, by nesting level, innermost first (dotty's
//! `ContextualImplicits`: `level` and `combineEligibles`).

use super::super::Typer;
use super::candidates::{Candidate, implicit_members, imported_implicits, this_implicit_members};
use super::too_late::comes_too_late;
use crate::ctx::{Ctx, Level, LevelId};
use tasty::nme;
use tasty::{SymbolId, Type};

/// How a level binds its implicits (dotty's `BindingPrec`).
#[derive(Clone, Copy, PartialEq)]
enum Prec {
    WildImport,
    NamedImport,
    Definition,
}

impl Prec {
    fn beats(self, other: Prec) -> bool {
        self == Prec::Definition || self == Prec::NamedImport && other == Prec::WildImport
    }
}

/// The implicits of one level by the names they are visible under, how it binds them, and the
/// owner of its context.
struct Refs {
    cands: Vec<(Candidate, String)>,
    prec: Prec,
    owner: Option<SymbolId>,
}

/// A visible implicit, its name and its nesting level.
type Leveled = (Candidate, String, u32);

/// Whether a candidate is eligible, asked with the typer.
type Eligible<'e, 'a> = dyn FnMut(&mut Typer<'a>, Candidate) -> bool + 'e;

impl<'a> Typer<'a> {
    /// The implicits visible in `ctx` that do not come too late, grouped by nesting level,
    /// innermost first; an inner one hides outer ones of its name if `eligible` accepts it, at
    /// the same level only if it binds as strongly. `Err` if seeing them needs a symbol whose
    /// completion is in progress (dotty's cyclic reference).
    pub fn contextual_implicits(
        &mut self,
        ctx: Ctx,
        eligible: &mut Eligible<'_, 'a>,
    ) -> Result<Vec<Vec<Candidate>>, &'static str> {
        let mut chain = Vec::new();
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            chain.extend(level_refs(self, l)?);
            level = self.levels.outer(l);
        }
        let mut seen: Vec<Leveled> = Vec::new();
        let mut outer: Option<(u32, Prec, Option<SymbolId>)> = None;
        for refs in chain.into_iter().rev() {
            let level = match outer {
                Some((n, _, o)) if refs.prec != Prec::Definition && o.is_some() && refs.owner == o => n,
                Some((n, ..)) => n + 1,
                None => 1,
            };
            let same_level_beaten = outer.is_some_and(|(n, p, _)| n == level && p.beats(refs.prec));
            seen = combined(self, refs.cands, seen, (level, same_level_beaten), eligible);
            outer = Some((level, refs.prec, refs.owner));
        }
        seen.retain(|(c, ..)| !comes_too_late(self, c.sym, ctx));
        let mut groups: Vec<(u32, Vec<Candidate>)> = Vec::new();
        for (c, _, level) in seen {
            match groups.iter_mut().find(|(l, _)| *l == level) {
                Some((_, g)) => g.push(c),
                None => groups.push((level, vec![c])),
            }
        }
        groups.sort_by_key(|g| std::cmp::Reverse(g.0));
        Ok(groups.into_iter().map(|(_, g)| g).collect())
    }
}

/// `own` at `level` combined with the implicits `outer` of the levels outside it (dotty's
/// `combineEligibles`).
fn combined<'a>(
    t: &mut Typer<'a>,
    own: Vec<(Candidate, String)>,
    outer: Vec<Leveled>,
    (level, beaten): (u32, bool),
    eligible: &mut Eligible<'_, 'a>,
) -> Vec<Leveled> {
    let mut names: Vec<String> = Vec::new();
    for (c, n) in &own {
        if !names.contains(n) && outer.iter().any(|(_, m, _)| m == n) && eligible(t, *c) {
            names.push(n.clone());
        }
    }
    let own = own.into_iter().map(|(c, n)| (c, n, level));
    if !beaten {
        return own.chain(outer.into_iter().filter(|(_, n, _)| !names.contains(n))).collect();
    }
    let shadowed = names.clone();
    let mut kept_outer = Vec::new();
    for (c, n, l) in outer {
        if names.contains(&n) {
            if l != level || !eligible(t, c) {
                continue;
            }
            names.retain(|m| *m != n);
        }
        kept_outer.push((c, n, l));
    }
    own.filter(|(_, n, _)| !shadowed.contains(n) || names.contains(n)).chain(kept_outer).collect()
}

/// The implicits level `l` makes visible, if any.
fn level_refs(t: &mut Typer<'_>, l: LevelId) -> Result<Option<Refs>, &'static str> {
    let refs = match t.levels.get(l) {
        Level::Scope(entries) => {
            let entries = entries.clone();
            let owner = entries.first().and_then(|&(_, s)| t.u.symbol(s).owner);
            let cands = entries
                .into_iter()
                .filter(|&(_, s)| t.u.symbol(s).is_implicit())
                .map(|(n, sym)| (Candidate { sym, prefix: None }, n))
                .collect();
            Refs { cands, prec: Prec::Definition, owner }
        }
        &Level::Members(owner) if t.u.symbol(owner).is_class() => {
            let this = crate::defn::type_ref(t.u, owner);
            let prefix = Some(t.u.intern_type(Type::This(this)));
            let members = this_implicit_members(t, owner);
            let cands = members
                .into_iter()
                .map(|sym| (Candidate { sym, prefix }, t.u.symbol(sym).name.clone()))
                .collect();
            Refs { cands, prec: Prec::Definition, owner: Some(owner) }
        }
        &Level::Members(pkg) if t.u.symbol(pkg).is_package() => {
            let members = match t.package_implicits.get(&pkg) {
                Some(members) => members.clone(),
                None => package_implicits(t, pkg)?,
            };
            let mut cands = Vec::new();
            for (c, sym) in members {
                let this = crate::defn::type_ref(t.u, c);
                let prefix = Some(t.u.intern_type(Type::This(this)));
                cands.push((Candidate { sym, prefix }, t.u.symbol(sym).name.clone()));
            }
            Refs { cands, prec: Prec::Definition, owner: Some(pkg) }
        }
        Level::Members(_) => return Ok(None),
        &Level::Import(i) => {
            let cands = imported_implicits(t, i)?;
            let Some(info) = t.import_info(i) else { return Ok(None) };
            let wildcard = info.selectors.iter().any(|s| s.name == nme::WILDCARD || s.name.is_empty());
            let prec = if wildcard { Prec::WildImport } else { Prec::NamedImport };
            Refs { cands, prec, owner: info.owner }
        }
    };
    Ok((!refs.cands.is_empty()).then_some(refs))
}

/// The implicit members of the package objects of `pkg`, by their objects, recorded in
/// `Typer::package_implicits`.
fn package_implicits(t: &mut Typer<'_>, pkg: SymbolId) -> Result<Vec<(SymbolId, SymbolId)>, &'static str> {
    let mut members = Vec::new();
    let script =
        t.u.symbol(pkg).decls.clone().into_iter().filter(|&c| {
            t.syms.is_script(c) && crate::defn::is_named_package_object(t.u, c) && t.u.is_class(c)
        });
    let mut objects: Vec<SymbolId> = script.collect();
    for c in t.u.package_objects(pkg).map_err(|_| "package object that cannot be read")? {
        if !objects.contains(&c) {
            objects.push(c);
        }
    }
    for c in objects {
        members.extend(implicit_members(t, c, true).into_iter().map(|m| (c, m)));
    }
    t.package_implicits.insert(pkg, members.clone());
    Ok(members)
}
