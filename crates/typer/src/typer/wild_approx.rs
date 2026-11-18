// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Expected types with their open type variables approximated by wildcards (dotty's
//! `wildApprox`), as transparent inline methods constrain their results.

use super::Typer;
use super::type_ops::TypeOps;
use crate::constraint::Constraint;
use syntax::config::SourceVersion;
use tasty::tags::INLINE;
use tasty::{SymbolId, Type, TypeId, TypeVars, Universe, VarId};

impl Typer<'_> {
    /// The type that constrains `result`, the result of method `meth`: `pt`, approximated by
    /// wildcards if `meth` is a transparent inline method, an inline method returning a match
    /// alias, or any inline method below Scala 3.4, unless the code being typed is in an inline
    /// method (dotty's `constrainResult(meth, mt, pt)`, `Inlines.isInlineable`).
    pub fn result_pt(&mut self, meth: Option<SymbolId>, result: TypeId, pt: TypeId) -> TypeId {
        let Some(m) = meth else { return pt };
        if self.u.comparison_owner().is_some_and(|o| self.enclosing_inline(o).is_some()) {
            return pt;
        }
        let approximated = self.is_transparent_inline(m)
            || self.u.symbol(m).flags.has(INLINE)
                && (!self.source_version.is_at_least(SourceVersion::V3_4)
                    || self.u.underlying_match(result).is_some());
        if approximated { self.types().wild_approx(pt) } else { pt }
    }
}

impl TypeOps<'_> {
    /// `t` with its uninstantiated type variables replaced by wildcards of their bounds, those
    /// approximated in turn; a variable within its own bounds by an unbounded wildcard.
    pub fn wild_approx(&mut self, t: TypeId) -> TypeId {
        let t = self.zonk(t);
        approx(self.u, self.constraint, t, &mut Vec::new())
    }
}

fn approx(u: &mut Universe, constraint: &Constraint, t: TypeId, seen: &mut Vec<VarId>) -> TypeId {
    u.map_type(t, &mut |u, part| {
        let v = u.var(part)?;
        if let Some(inst) = constraint.instance(v) {
            return Some(approx(u, constraint, inst, seen));
        }
        if seen.contains(&v) {
            return Some(u.intern_type(Type::Wildcard(None)));
        }
        seen.push(v);
        let (lo, hi) = constraint.bounds(v);
        let (lo, hi) = (approx(u, constraint, lo, seen), approx(u, constraint, hi, seen));
        seen.pop();
        let bounds = u.intern_type(Type::Bounds(lo, hi));
        Some(u.intern_type(Type::Wildcard(Some(bounds))))
    })
}
