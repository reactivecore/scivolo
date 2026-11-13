// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/GadtConstraint.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The type parameters a case's pattern may narrow and their current bounds.

use super::gadt_cases::Gadt;
use super::type_ops::TypeOps;
use crate::ctx::Ctx;
use tasty::{Kind, SymbolId, TypeId, Universe};

/// Finds the type `t` refers to plainly, as `T`, or seen from `this` of its owner.
pub fn plain_type_ref(u: &mut Universe, t: TypeId) -> Option<SymbolId> {
    let (prefix, s) = u.sym_ref(t)?;
    let Some(p) = prefix else { return Some(s) };
    let c = u.this_of(p)?;
    (u.class_symbol(c) == u.symbol(s).owner).then_some(s)
}

impl Gadt {
    /// Tells whether a case may narrow `s`: a type parameter of a method enclosing `ctx`'s owner,
    /// of a polymorphic function value's closure enclosing it, or of a class enclosing one
    /// (dotty's `addToConstraint` in `typedDefDef`), or a type bound by a pattern of a case being
    /// typed.
    pub fn is_symbol(&self, u: &mut Universe, s: SymbolId, ctx: Ctx) -> bool {
        if self.bound.contains(&s) {
            return true;
        }
        let Some(of) = u.symbol(s).owner.filter(|_| u.symbol(s).kind == Kind::TypeParam) else {
            return false;
        };
        let poly = self.poly_owners.contains(&of).then(|| u.symbol(of).owner);
        let (mut owner, mut in_method) = (Some(ctx.owner), false);
        while let Some(o) = owner {
            match u.symbol(o).kind {
                Kind::Def => in_method = true,
                Kind::Class if in_method => {}
                _ => {
                    owner = u.symbol(o).owner;
                    continue;
                }
            }
            if o == of || poly.is_some_and(|p| u.symbol(o).owner == p) {
                return true;
            }
            owner = u.symbol(o).owner;
        }
        false
    }
}

impl TypeOps<'_> {
    /// Tells whether `t` refers to a type variable.
    pub fn has_type_var(&mut self, t: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, p| {
            found |= u.var(p).is_some();
            None
        });
        found
    }

    /// Tells whether `t` refers to type `part`.
    pub fn refers_to_type(&mut self, t: TypeId, part: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |_, p| {
            found |= p == part;
            None
        });
        found
    }
}
