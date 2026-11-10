// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/RefChecks.scala
//! dotty's `checkAllOverrides` step 4, a member with an `override` modifier overrides some member,
//! where no base member matches it without differing in a repeated parameter.

use super::super::Typer;
use super::{OverrideChecks, overridable};
use syntax::ast::flags as f;
use tasty::{SymbolId, Universe};

impl OverrideChecks {
    /// Whether a member of the class with an `override` modifier overrides nothing in its base
    /// classes or those of its self type, as `override def g = 1` where no base class has a `g`.
    pub fn overrides_nothing(&self, t: &mut Typer<'_>) -> bool {
        let mut bases = self.bases.clone();
        bases.extend(t.u.self_type_bases(self.cls));
        bases.retain(|&b| b != self.cls);
        let own: Vec<SymbolId> = t.u.symbol(self.cls).decls.clone();
        own.into_iter().any(|d| {
            let flags = t.syms.flags.get(&d).copied().unwrap_or_default();
            (flags.is(f::OVERRIDE) || flags.is(f::ABS_OVERRIDE))
                && !flags.is(f::SYNTHETIC)
                && self.matches_nothing(t, &bases, d)
        })
    }

    /// Whether member `d` of the class matches no member of `bases`; a `getClass` of a primitive
    /// value class matches `Any`'s.
    fn matches_nothing(&self, t: &mut Typer<'_>, bases: &[SymbolId], d: SymbolId) -> bool {
        let name = t.u.symbol(d).name.clone();
        if !t.u.symbol(d).kind.is_term() {
            return !bases.iter().any(|&b| {
                t.u.symbol(b).decls.iter().any(|&m| {
                    let s = t.u.symbol(m);
                    s.name == name && !s.kind.is_term() && !s.is_private()
                })
            });
        }
        let named: Vec<SymbolId> = bases.iter().flat_map(|&b| overridable(t.u, b, &name)).collect();
        let nothing = named.into_iter().all(|m| {
            t.complete(m);
            !t.same_member(self.site, d, m) || incompatible_repeated_param(t, d, m)
        });
        nothing && !(name == "getClass" && is_primitive_value_class(t.u, self.cls))
    }
}

fn is_primitive_value_class(u: &Universe, c: SymbolId) -> bool {
    tasty::PRIMITIVE_VALUE_CLASSES.contains(&u.full_name(c).as_str())
}

/// Whether neither of `a` and `b` is a Java member and in some parameter clause their last
/// parameters differ in being repeated.
fn incompatible_repeated_param(t: &mut Typer<'_>, a: SymbolId, b: SymbolId) -> bool {
    if t.is_java_member(a) || t.is_java_member(b) {
        return false;
    }
    let (ia, ib) = (t.u.info(a), t.u.info(b));
    let (ca, cb) = (t.u.param_clauses(ia), t.u.param_clauses(ib));
    ca.into_iter().zip(cb).any(|((_, x), (_, y))| match (x.last(), y.last()) {
        (Some(&x), Some(&y)) => t.repeated_param_elem(x).is_some() != t.repeated_param_elem(y).is_some(),
        _ => false,
    })
}
