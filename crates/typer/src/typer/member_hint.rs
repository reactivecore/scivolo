// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/reporting/DidYouMean.scala
//   compiler/src/dotty/tools/dotc/reporting/messages.scala
//! The "did you mean" hint of dotty's `NotAMember` (`DidYouMean.memberCandidates`, `closestTo`
//! and `didYouMean`).

use super::Typer;
use super::did_you_mean::{OBJECT_MEMBERS, close, distance};
use crate::ctx::Ctx;
use tasty::tags::{PRIVATE, SYNTHETIC};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The hint for missing member `missing`, a type if `is_type`, of class `class` selected from
    /// a path of type `site` in `ctx`, empty if none; `None` where it is not certain.
    pub fn member_hint(
        &mut self,
        class: SymbolId,
        site: TypeId,
        missing: &str,
        is_type: bool,
        ctx: Ctx,
    ) -> Option<String> {
        let units: Vec<u16> = missing.encode_utf16().collect();
        let mut found = Vec::new();
        for base in self.known_bases(class)? {
            for d in self.u.decls(base).ok()? {
                let module_class = self.u.is_module_class(d);
                let s = self.u.symbol(d);
                let kind_ok = match is_type {
                    true => s.kind.is_type(),
                    false => s.kind.is_term() || s.kind == Kind::Class && !module_class,
                };
                let excluded = s.name == "<init>" || s.flags.has(SYNTHETIC) || s.flags.has(PRIVATE);
                if !kind_ok || excluded || !close(&units, s.name.trim_end_matches('$')) {
                    continue;
                }
                let plain_class = is_type && !module_class;
                let odd_kind =
                    matches!(s.kind, Kind::Param | Kind::TypeParam) || s.kind == Kind::Class && !plain_class;
                if odd_kind || !close(&units, &s.name) {
                    return None;
                }
                let d_units = distance(&units, &s.name.encode_utf16().collect::<Vec<_>>());
                if d_units == 0 {
                    return None;
                }
                found.push((d_units, s.name.clone(), d));
            }
        }
        let known: Vec<&str> = found.iter().map(|(_, n, _)| n.as_str()).collect();
        if OBJECT_MEMBERS.split_whitespace().any(|n| close(&units, n) && !known.contains(&n)) {
            return None;
        }
        found.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        let mut qualifying = Vec::new();
        for (d, name, sym) in found {
            if self.is_accessible_from(sym, Some(site), ctx) {
                qualifying.push((d, name));
            }
        }
        let Some((best, first)) = qualifying.first().cloned() else { return Some(String::new()) };
        let prefix = self.hint_prefix(site)?;
        let alts: Vec<String> = qualifying[1..]
            .iter()
            .take_while(|(d, _)| *d == best)
            .map(|(_, n)| format!("{prefix}{n}"))
            .collect();
        Some(super::errors::did_you_mean(&format!("{prefix}{first}"), &alts))
    }

    /// What dotty's hint writes before a suggested member of `site`: a named type's name and a
    /// dot; `None` for other types.
    fn hint_prefix(&mut self, site: TypeId) -> Option<String> {
        let name = match self.u.types().get(site) {
            Type::TermRef(_, n) | Type::TypeRef(_, n) => n.clone(),
            &Type::Sym(_, s) => self.u.symbol(s).name.clone(),
            _ => return None,
        };
        (!name.ends_with('$')).then(|| format!("{name}."))
    }
}
