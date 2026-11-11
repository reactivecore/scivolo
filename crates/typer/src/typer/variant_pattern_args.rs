// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/PatternTypeConstrainer.scala
//! Selectors against which a typed pattern constrains its bound types at most by variant
//! arguments (dotty's `constrainSimplePatternType` for such a scrutinee).

use super::Typer;
use super::dependent_results::has_wildcard;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Gives each of `syms`, the types pattern type `pat` binds, its bounds against `sel`, a class
    /// applied without wildcards at variant parameters only: those of the same class bound a
    /// pattern-bound argument by the selector's at its variance, any other pattern none; whether
    /// it did, `false` for other selectors and arguments.
    pub fn bound_by_variant_args(&mut self, pat: TypeId, sel: TypeId, syms: &[SymbolId]) -> bool {
        let sel = self.strip_null(sel);
        let sel = self.dealiased(sel);
        let sel = self.u.widen(sel);
        let Some((tycon, sargs)) = self.u.applied(sel).map(|(tycon, args)| (tycon, args.to_vec())) else {
            return false;
        };
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
        let Some(pargs) = self.u.applied(pat).map(|(_, args)| args.to_vec()) else { return false };
        if pargs.len() != sargs.len() {
            return false;
        }
        let mut bounds: Vec<(SymbolId, TypeId, TypeId)> = Vec::new();
        for ((&p, &s), &v) in pargs.iter().zip(&sargs).zip(&variances) {
            let s = self.u.dealias(s);
            match self.u.sym_ref(p) {
                Some((None, b)) if syms.contains(&b) => {
                    let info = self.u.info(b);
                    let Some((lo, hi)) = self.u.bounds(info) else { return false };
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

    /// Whether pattern type `pat`, or its base type at `sel`'s class, and `sel` apply the same
    /// class, each of `syms` an argument of `pat` only where `sel` has an unbounded wildcard,
    /// which constrains none of them; for an abstract `sel`, its upper bound.
    pub fn only_against_wildcards(&mut self, pat: TypeId, sel: TypeId, syms: &[SymbolId]) -> bool {
        let sel = self.dealiased(sel);
        let sel = self.u.widen(sel);
        if let Some(hi) = self.abstract_upper(sel) {
            return self.only_against_wildcards(pat, hi, syms);
        }
        let base = match (self.u.class_symbol(pat), self.u.class_symbol(sel)) {
            (Some(p), Some(s)) if p != s && self.u.derives_from(p, s) => self.u.base_type(pat, s),
            _ => Some(pat),
        };
        let (Some((pc, pargs)), Some((sc, sargs))) = (
            base.and_then(|b| self.u.applied(b)).map(|(tycon, args)| (tycon, args.to_vec())),
            self.u.applied(sel).map(|(tycon, args)| (tycon, args.to_vec())),
        ) else {
            return false;
        };
        let same = self.u.class_symbol(pc).is_some_and(|c| self.u.class_symbol(sc) == Some(c));
        let b = self.u.defn();
        let (any, nothing) = (b.any, b.nothing);
        same && pargs.len() == sargs.len()
            && pargs.iter().zip(&sargs).all(|(&p, &s)| match self.u.sym_ref(p) {
                Some((None, b)) if syms.contains(&b) => match self.u.bounds(s) {
                    Some((lo, hi)) => {
                        self.u.class_symbol(lo) == Some(nothing) && self.u.class_symbol(hi) == Some(any)
                    }
                    None => false,
                },
                _ => !self.refers_to(p, syms),
            })
    }

    /// Whether `sel` is a class type whose base classes have no type parameters, or an abstract
    /// type bounded by one, against which a pattern's type constrains none of its bound types.
    pub fn has_unapplied_bases(&mut self, sel: TypeId) -> bool {
        let sel = self.dealiased(sel);
        let sel = self.u.widen(sel);
        if let Some(hi) = self.abstract_upper(sel) {
            return self.has_unapplied_bases(hi);
        }
        let Some(c) = self.u.sym_of(sel) else { return false };
        self.u.is_class(c) && self.u.linearization(c).into_iter().all(|b| self.u.type_params(b).is_empty())
    }

    /// The upper bound of abstract type `sel` without GADT bounds, other than `sel`.
    fn abstract_upper(&mut self, sel: TypeId) -> Option<TypeId> {
        let (pre, c) = self.u.sym_ref(sel)?;
        if !matches!(self.u.symbol(c).kind, Kind::TypeParam | Kind::Type) || self.u.gadt_bounds(c).is_some() {
            return None;
        }
        let info = match pre {
            Some(p) => self.u.member_info(p, c),
            None => self.u.info(c),
        };
        let (_, hi) = self.u.bounds(info)?;
        (hi != sel).then_some(hi)
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
