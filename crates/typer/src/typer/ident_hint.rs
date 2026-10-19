//! The message of dotty's `MissingIdent`: the wrong-kind note or the "did you mean" hint.

use super::Typer;
use super::did_you_mean::{close, distance};
use super::errors;
use crate::ctx::Ctx;
use syntax::diag::Message;
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// `MissingIdent` for `missing`, a type if `is_type`, not the function of an application, in
    /// `ctx`; `None` where its text is not certain.
    pub fn missing_ident(&mut self, missing: &str, is_type: bool, ctx: Ctx) -> Option<Message> {
        let units: Vec<u16> = missing.encode_utf16().collect();
        let near = |n: &str| close(&units, n) || close(&units, n.trim_end_matches('$'));
        let b = self.bindings(ctx)?;
        let all = self.candidates(ctx)?;
        let placed = |n: &String| b.found.iter().any(|(f, ..)| f == n) || b.left_out.contains(n);
        let unplaced = all.iter().filter(|n| !placed(n));
        let mut doubtful: Vec<String> =
            b.unknown.iter().chain(unplaced).filter(|n| near(n)).cloned().collect();
        let (mut wrong, mut module) = (false, false);
        for &(ref name, sym, _) in &b.found {
            let shown = if self.u.is_module_class(sym) { name.trim_end_matches('$') } else { name.as_str() };
            if shown == missing && self.kind_ok(sym, !is_type)? {
                wrong = true;
                module |= self.u.is_module(sym);
            }
        }
        if wrong {
            let maybe_module = is_type && !module && doubtful.iter().any(|n| n == missing);
            return (!maybe_module).then(|| errors::wrong_kind(missing, is_type, module));
        }
        let mut found = Vec::new();
        for (name, sym, site) in b.found {
            if !near(&name) {
                continue;
            }
            match self.kind_ok(sym, is_type) {
                Some(false) => {}
                Some(true) if close(&units, &name) => {
                    match distance(&units, &name.encode_utf16().collect::<Vec<_>>()) {
                        0 => return None,
                        d => found.push((d, name, sym, site)),
                    }
                }
                _ => doubtful.push(name),
            }
        }
        found.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        let mut qualifying: Vec<(usize, String)> = Vec::new();
        for (d, name, sym, site) in found {
            if self.is_accessible_from(sym, site, ctx) {
                qualifying.push((d, name));
            }
        }
        let Some((best, first)) = qualifying.first().cloned() else {
            return doubtful.is_empty().then(|| errors::not_found(missing, is_type, ""));
        };
        let alts: Vec<String> =
            qualifying[1..].iter().take_while(|(d, _)| *d == best).map(|(_, n)| n.clone()).collect();
        let dist = |n: &str| distance(&units, &n.trim_end_matches('$').encode_utf16().collect::<Vec<_>>());
        let extra: Vec<&String> = doubtful.iter().filter(|n| **n != first && dist(n) <= best).collect();
        let others = alts.iter().filter(|n| **n != first).count();
        if extra.iter().any(|n| !alts.contains(*n)) || !extra.is_empty() && others + extra.len() > 3 {
            return None;
        }
        Some(errors::not_found(missing, is_type, &errors::did_you_mean(&first, &alts)))
    }

    /// Whether binding `sym` is of the kind dotty's hint looks for; `None` where the hint may show
    /// it otherwise or leave it out, as an object's class.
    fn kind_ok(&mut self, sym: SymbolId, is_type: bool) -> Option<bool> {
        match (is_type, self.u.symbol(sym).kind) {
            (true, Kind::Class) if self.u.is_module_class(sym) => None,
            (false, Kind::Package) => None,
            (true, k) => Some(k.is_type()),
            (false, k) => Some(k.is_term()),
        }
    }
}
