//! The message of dotty's `MissingIdent`: the wrong-kind note or the "did you mean" hint.

use super::Typer;
use super::did_you_mean::{close, distance};
use super::errors;
use crate::ctx::Ctx;
use syntax::diag::Message;
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// `MissingIdent` for `missing`, a type if `is_type`, the function of an application if
    /// `applied`, in `ctx`; `None` where its text is not certain.
    pub fn missing_ident(
        &mut self,
        missing: &str,
        is_type: bool,
        applied: bool,
        ctx: Ctx,
    ) -> Option<Message> {
        let units: Vec<u16> = missing.encode_utf16().collect();
        let near = |n: &str| close(&units, n) || close(&units, n.trim_end_matches('$'));
        let b = self.bindings(ctx)?;
        let all = self.candidates(ctx)?;
        let placed = |n: &String| b.found.iter().any(|(f, ..)| f == n) || b.left_out.contains(n);
        let unplaced = all.iter().filter(|n| !placed(n));
        let mut doubtful: Vec<String> = b
            .unknown
            .iter()
            .chain(unplaced)
            .filter(|n| near(n))
            .map(|n| n.trim_end_matches('$').to_owned())
            .collect();
        let (mut wrong, mut module) = (false, false);
        for &(ref name, sym, _) in &b.found {
            if name == missing && self.kind_ok(sym, !is_type, applied)? {
                wrong = true;
                module |= self.u.is_module(sym);
            }
        }
        if wrong {
            return Some(errors::wrong_kind(missing, is_type, module));
        }
        let mut found = Vec::new();
        for (name, sym, site) in b.found {
            if !near(&name) {
                continue;
            }
            match self.kind_ok(sym, is_type, applied) {
                None => doubtful.push(name),
                Some(false) => {}
                Some(true) => match distance(&units, &name.encode_utf16().collect::<Vec<_>>()) {
                    0 => return None,
                    d => found.push((d, name, sym, site)),
                },
            }
        }
        found.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        let mut qualifying: Vec<(usize, String)> = Vec::new();
        for (d, name, sym, site) in found {
            if self.is_accessible_from(sym, site, ctx) && !qualifying.iter().any(|(_, n)| *n == name) {
                qualifying.push((d, name));
            }
        }
        let best = qualifying.first().map_or(usize::MAX, |q| q.0);
        let shown = |n: &str| qualifying.iter().any(|(_, q)| q == n);
        let dist = |n: &str| distance(&units, &n.trim_end_matches('$').encode_utf16().collect::<Vec<_>>());
        if doubtful.iter().any(|n| dist(n) <= best && !shown(n)) {
            return None;
        }
        let hint = match qualifying.first() {
            None => String::new(),
            Some((best, first)) => {
                let alts: Vec<&str> = qualifying[1..]
                    .iter()
                    .filter(|(d, _)| d == best)
                    .take(3)
                    .map(|(_, n)| n.as_str())
                    .collect();
                let suffix = match alts.is_empty() {
                    true => String::new(),
                    false => format!(" or perhaps {}?", alts.join(" or ")),
                };
                format!(" - did you mean {first}?{suffix}")
            }
        };
        Some(errors::not_found(missing, is_type, &hint))
    }

    /// Whether binding `sym` is of the kind dotty's hint looks for; `None` where the hint may show
    /// it otherwise or leave it out, as a class for an applied term.
    fn kind_ok(&mut self, sym: SymbolId, is_type: bool, applied: bool) -> Option<bool> {
        let module_class = self.u.is_module_class(sym);
        match (is_type, self.u.symbol(sym).kind) {
            (true, Kind::Class) if module_class => None,
            (false, Kind::Class) if applied && !module_class => None,
            (false, Kind::Class) => Some(false),
            (false, Kind::Package) => None,
            (true, k) => Some(k.is_type()),
            (false, k) => Some(k.is_term()),
        }
    }
}
