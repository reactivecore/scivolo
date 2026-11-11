// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/VarianceChecker.scala

use super::{BIVARIANT, COVARIANT, INVARIANT, Misplaced, VarianceOps, declared, flip};
use syntax::ast::flags as f;
use tasty::{Kind, SymbolId, nme};

impl VarianceOps<'_> {
    /// Determines the variance of an occurrence of type parameter `tvar` in the type of `base`,
    /// seen from the definition of `tvar`: `v` changed along the owners of `base` up to the owner
    /// of `tvar` (dotty's `relativeVariance`). A parameter of a method of class `C[+A]` is seen
    /// contravariantly; a private local member's type is not checked, so bivariant.
    pub fn relative_variance(&mut self, tvar: SymbolId, mut base: SymbolId, mut v: u8) -> u8 {
        let towner = self.u.symbol(tvar).owner;
        loop {
            if Some(base) == towner {
                return v;
            }
            let Some(owner) = self.u.symbol(base).owner else { return BIVARIANT };
            let (kind, owner_kind) = (self.u.symbol(base).kind, self.u.symbol(owner).kind);
            let param = matches!(kind, Kind::Param | Kind::TypeParam);
            if param && owner_kind.is_term() && !self.syms.flags_of(owner).is_all_of(f::PRIVATE_LOCAL) {
                let outer = self.u.symbol(owner).owner;
                let ctor = self.u.symbol(owner).name == nme::CONSTRUCTOR;
                let outer = if ctor { outer.and_then(|o| self.u.symbol(o).owner) } else { outer };
                let Some(outer) = outer else { return BIVARIANT };
                (base, v) = (outer, flip(v));
            } else if owner_kind.is_term()
                || owner_kind == Kind::Package
                || self.syms.flags_of(base).is_all_of(f::PRIVATE_LOCAL)
            {
                return BIVARIANT;
            } else if kind == Kind::Type
                && (self.syms.flags_of(base).is(f::OPAQUE) || self.u.alias_of(base).is_some())
            {
                (base, v) = (owner, 0);
            } else {
                base = owner;
            }
        }
    }

    /// `tvar` with the variance required of it where it occurs at variance `v` in the type of
    /// `base`, if its own is not that (dotty's `checkVarianceOfSymbol`).
    pub fn misplaced_symbol(&mut self, tvar: SymbolId, base: SymbolId, v: i8) -> Option<Misplaced> {
        let relative = self.relative_variance(tvar, base, COVARIANT);
        if relative == BIVARIANT {
            return None;
        }
        let required = match v {
            1 => relative,
            -1 => flip(relative),
            _ => INVARIANT,
        };
        (declared(self.u, tvar) & required == 0).then_some(Misplaced::Class(tvar, required))
    }
}
