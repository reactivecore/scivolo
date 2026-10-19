// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/reporting/DidYouMean.scala
//! The bindings dotty's "did you mean" hint of `MissingIdent` considers (dotty's
//! `DidYouMean.inScopeCandidates`, `memberCandidates`).

use super::Typer;
use super::imports::ImportEntry;
use super::java_classes::java_classes;
use crate::ctx::{Ctx, Level};
use tasty::tags::{PRIVATE, SYNTHETIC};
use tasty::{Kind, SymbolId, TypeId};

/// The bindings in scope with their prefixes, the names left out of them, and names that may be
/// bound in ways not known.
#[derive(Default)]
pub struct Bindings {
    pub found: Vec<(String, SymbolId, Option<TypeId>)>,
    pub left_out: Vec<String>,
    pub unknown: Vec<String>,
}

impl Typer<'_> {
    /// The bindings of all levels of `ctx`; `None` where they are not all known.
    pub fn bindings(&mut self, ctx: Ctx) -> Option<Bindings> {
        let mut b = Bindings::default();
        let mut level = Some(ctx.level);
        while let Some(l) = level {
            match self.levels.get(l) {
                Level::Scope(entries) => {
                    for (n, s) in entries.clone() {
                        let sym = self.u.symbol(s);
                        match sym.name == "<init>" || sym.flags.has(SYNTHETIC) {
                            true => b.left_out.push(n),
                            false => b.found.push((n, s, None)),
                        }
                    }
                }
                &Level::Members(p) if self.u.symbol(p).kind == Kind::Package => {
                    if p != self.u.root() {
                        return None;
                    }
                    self.member_bindings(p, None, &mut b)?;
                }
                &Level::Members(c) => self.member_bindings(c, None, &mut b)?,
                &Level::Import(i) => self.import_bindings(i, &mut b)?,
            }
            level = self.levels.outer(l);
        }
        Some(b)
    }

    /// Adds the bindings import `i` makes visible to `b`.
    fn import_bindings(&mut self, i: usize, b: &mut Bindings) -> Option<()> {
        if self.unimported(i).is_some() {
            return None;
        }
        let ImportEntry::Done(info) = &self.imports[i] else { return None };
        let qual = info.qual;
        let wildcard = info.selectors.iter().any(|s| s.name == "_" || s.name.is_empty());
        let selectors: Vec<(String, String)> =
            info.selectors.iter().map(|s| (s.name.clone(), s.rename.clone())).collect();
        let site = self.import_site_class(qual)?;
        let mut members = Bindings::default();
        self.member_bindings(site, Some(qual), &mut members)?;
        b.left_out.append(&mut members.left_out);
        b.unknown.append(&mut members.unknown);
        for (n, s, p) in members.found {
            let excluded = selectors.iter().any(|(name, rename)| *name == n && rename != name);
            let mut bound = wildcard && !excluded;
            if bound {
                b.found.push((n.clone(), s, p));
            }
            for (_, rename) in selectors.iter().filter(|(name, _)| *name == n) {
                if rename != "_" {
                    b.found.push((rename.clone(), s, p));
                    bound = true;
                }
            }
            if !bound {
                b.left_out.push(n);
            }
        }
        Some(())
    }

    /// Adds the members of package or class `site` that dotty's hint considers to `b`, with
    /// prefix `prefix` (dotty's `memberCandidates`).
    fn member_bindings(&mut self, site: SymbolId, prefix: Option<TypeId>, b: &mut Bindings) -> Option<()> {
        let package = self.u.symbol(site).kind == Kind::Package;
        let owners = match package {
            true => vec![site],
            false => self.known_bases(site)?,
        };
        if package {
            let full = self.u.full_name(site);
            b.unknown.extend(java_classes(&full).map(str::to_owned));
        }
        for owner in owners {
            for d in self.u.decls(owner).ok()? {
                if package && self.is_named_package_object(d) {
                    let inner = self.u.decls(d).ok()?;
                    b.unknown.extend(inner.iter().map(|&m| self.u.symbol(m).name.clone()));
                }
                let s = self.u.symbol(d);
                match s.name == "<init>" || s.flags.has(SYNTHETIC) || s.flags.has(PRIVATE) {
                    true => b.left_out.push(s.name.clone()),
                    false => b.found.push((s.name.clone(), d, prefix)),
                }
            }
        }
        Some(())
    }
}
