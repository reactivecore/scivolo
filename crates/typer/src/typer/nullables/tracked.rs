// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Nullables.scala
//! The references flow typing tracks (dotty's `TrackedRef`, `usedOutOfOrder`).

use super::super::Typer;
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use tasty::{Kind, SymbolId, Type, TypeId, tags};

impl Typer<'_> {
    pub fn term_ref_sym(&mut self, r: TypeId) -> Option<SymbolId> {
        match *self.u.types().get(r) {
            Type::Sym(_, s) if self.u.symbol(s).kind.is_term() => Some(s),
            _ => None,
        }
    }

    pub fn is_mutable_var(&self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        sym.kind == Kind::Val && sym.flags.has(tags::MUTABLE)
    }

    /// The type of `t` if it is tracked: a stable path or a local variable assigned only where its
    /// block reaches (dotty's `isTracked`).
    pub fn tracked(&mut self, t: TNodeId) -> Option<TypeId> {
        let r = self.tpd.tpe(t);
        let sym = self.term_ref_sym(r)?;
        if super::super::stability::is_stable_path(self.u, r) || self.is_null_stable_field(r, sym) {
            return Some(r);
        }
        self.assignment_spans(sym).is_some().then_some(r)
    }

    /// Whether `r` selects field `sym` annotated `@stableNull` from a stable prefix.
    fn is_null_stable_field(&mut self, r: TypeId, sym: SymbolId) -> bool {
        let Type::Sym(Some(p), _) = *self.u.types().get(r) else { return false };
        let s = self.u.symbol(sym);
        let field = s.kind == Kind::Val && s.owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Class);
        field
            && self.u.annotations(sym).iter().any(|&c| self.u.full_name(c) == "scala.annotation.stableNull")
            && super::super::stability::is_stable_path(self.u, p)
    }

    /// Whether a use of local variable `sym` in `ctx` is in a method, lazy value or class inside
    /// its owner.
    pub fn used_out_of_order(&self, sym: SymbolId, ctx: Ctx) -> bool {
        let Some(owner) = self.u.symbol(sym).owner else { return false };
        if !self.is_mutable_var(sym) || !self.u.symbol(owner).kind.is_term() {
            return false;
        }
        let mut s = Some(ctx.owner);
        while let Some(o) = s.filter(|&o| o != owner) {
            let os = self.u.symbol(o);
            if matches!(os.kind, Kind::Def | Kind::Class) || os.flags.has(tags::LAZY) {
                return true;
            }
            s = os.owner;
        }
        false
    }
}
