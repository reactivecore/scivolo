// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//! The bounds of the types a typed pattern binds against a class applied at variant parameters
//! only (dotty's `constrainSimplePatternType` for such a scrutinee).

use super::Typer;
use super::dependent_results::has_wildcard;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Gives each of `syms`, the types pattern type `pat` binds, its bounds against `sel`, a class
    /// applied without wildcards at variant parameters only: those of the same class bound a
    /// pattern-bound argument by the selector's at its variance, any other pattern none; whether
    /// it did, `false` for other selectors and arguments.
    pub fn bound_by_variant_args(&mut self, pat: TypeId, sel: TypeId, syms: &[SymbolId]) -> bool {
        let sel = self.strip_null(sel);
        let sel = self.dealiased(sel);
        let sel = self.u.widen(sel);
        let Type::Applied(tycon, sargs) = self.u.types().get(sel).clone() else { return false };
        if has_wildcard(self.u, sel) || self.is_opaque_ref(tycon) {
            return false;
        }
        let Some(class) = self.u.class_symbol(tycon) else { return false };
        let variances = self.u.variances(class);
        if variances.len() != sargs.len() || variances.contains(&0) {
            return false;
        }
        let pat = self.dealiased(pat);
        if self.u.class_symbol(pat) != Some(class) {
            return true;
        }
        let Type::Applied(_, pargs) = self.u.types().get(pat).clone() else { return false };
        if pargs.len() != sargs.len() {
            return false;
        }
        let mut bounds: Vec<(SymbolId, TypeId, TypeId)> = Vec::new();
        for ((&p, &s), &v) in pargs.iter().zip(&sargs).zip(&variances) {
            let part = self.u.types().get(p).clone();
            match part {
                Type::Sym(None, b) if syms.contains(&b) => {
                    let info = self.u.info(b);
                    let Type::Bounds(lo, hi) = *self.u.types().get(info) else { return false };
                    if bounds.iter().any(|&(x, ..)| x == b) {
                        return false;
                    }
                    let narrowed = match v > 0 {
                        true => self.narrower(hi, s, false),
                        false => self.narrower(lo, s, true),
                    };
                    let Some(n) = narrowed else { return false };
                    let (lo, hi) = if v > 0 { (lo, n) } else { (n, hi) };
                    if !self.u.is_subtype(lo, hi) {
                        return false;
                    }
                    bounds.push((b, lo, hi));
                }
                _ if self.refers_to(p, syms) => return false,
                _ => {}
            }
        }
        for (b, lo, hi) in bounds {
            let info = self.u.intern_type(Type::Bounds(lo, hi));
            self.u.set_info(b, info);
        }
        true
    }

    /// Of bound `old` and `new`, the one that implies the other, the lower bound by `lower`; `None`
    /// if neither does.
    fn narrower(&mut self, old: TypeId, new: TypeId, lower: bool) -> Option<TypeId> {
        let (sub, sup) = if lower { (old, new) } else { (new, old) };
        match (self.u.is_subtype(sub, sup), self.u.is_subtype(sup, sub)) {
            (true, _) => Some(new),
            (_, true) => Some(old),
            _ => None,
        }
    }
}
