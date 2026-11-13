// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/core/GadtConstraint.scala
//! Registering the type parameters a case may narrow in the GADT constraint.

use super::gadt_cases::Gadt;
use crate::ctx::Ctx;
use tasty::{Kind, Registered, SymbolId, Universe};

impl Gadt {
    /// Registers in the GADT constraint the type parameters a case at `ctx` may narrow that are
    /// not yet registered, in dotty's order: per enclosing method from the outermost, the type
    /// parameters of the classes between it and the next enclosing method from the outermost,
    /// then those of a polymorphic function value it is the body of, then its own; each
    /// method's at its nesting depth (dotty's `addToConstraint` in `typedDefDef`). Gives the
    /// level of the case, below every method.
    pub fn register(&self, u: &mut Universe, ctx: Ctx) -> u32 {
        let mut chain = Vec::new();
        let mut owner = Some(ctx.owner);
        while let Some(o) = owner {
            chain.push(o);
            owner = u.symbol(o).owner;
        }
        let (mut classes, mut level) = (Vec::new(), 0);
        for &o in chain.iter().rev() {
            match u.symbol(o).kind {
                Kind::Class => classes.push(o),
                Kind::Def => {
                    let mut params: Vec<SymbolId> = Vec::new();
                    for c in classes.drain(..) {
                        params.extend(u.type_params(c));
                    }
                    let site = u.symbol(o).owner;
                    let polys: Vec<SymbolId> =
                        self.poly_owners.iter().copied().filter(|&p| u.symbol(p).owner == site).collect();
                    for p in polys {
                        params.extend(u.type_params(p));
                    }
                    if !u.is_constructor(o) {
                        params.extend(u.type_params(o));
                    }
                    self.register_params(u, &params, level, ctx);
                    level += 1;
                }
                _ => {}
            }
        }
        level
    }

    /// Registers pattern-bound types `syms` one by one at the case's `level` (dotty's
    /// `newPatternBoundSymbol` with `addToGadt`).
    pub fn register_bound(&self, u: &mut Universe, syms: &[SymbolId], level: u32) {
        for &sym in syms {
            let info = u.info(sym);
            let Some((lo, hi)) = u.bounds(info).filter(|_| !u.gadt().contains(sym)) else { continue };
            u.gadt_register(&[Registered { sym, lo, hi }], level);
        }
    }

    /// Registers those of `params` a case at `ctx` may narrow and that are not registered yet,
    /// all at once, at `level`.
    fn register_params(&self, u: &mut Universe, params: &[SymbolId], level: u32, ctx: Ctx) {
        let mut registered = Vec::new();
        for &p in params {
            if u.gadt().contains(p) || !self.is_symbol(u, p, ctx) {
                continue;
            }
            let info = u.info(p);
            let Some((lo, hi)) = u.bounds(info) else { continue };
            registered.push(Registered { sym: p, lo, hi });
        }
        u.gadt_register(&registered, level);
    }
}
