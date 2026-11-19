// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! A parameterless alternative applied through the `apply` members of its result, when no method
//! takes the arguments (dotty's `applyMembers` and `retract` in `resolveOverloaded`).

use super::alternatives::{RESULT_APPLY, result_applies};
use super::poly_instances::mapped_info;
use super::{Resolution, Unchosen};
use crate::typer::Typer;
use tasty::SymbolId;

impl Resolution {
    /// Chooses the parameterless method of `methods` whose result has `apply` members taking as
    /// many arguments as `proto`, if it is the only one, as `v(x => x)` among `def v: Test0` and
    /// `def v[T]: Test1[Int]` with `Test1` taking a function, once the arguments select one of
    /// those members; refuses a result type that mentions the method's type parameters.
    pub fn result_apply_alternative(
        self,
        t: &mut Typer,
        methods: &[SymbolId],
        proto: usize,
    ) -> Result<SymbolId, Unchosen> {
        let n = t.protos[proto].args.len();
        let mut owners = Vec::new();
        for &s in methods {
            let tpe = t.ref_type(self.prefix, s);
            let info = t.method_info(tpe).unwrap_or(tpe);
            let info = mapped_info(t, info, s);
            if t.u.param_clauses(info).is_empty() {
                let applies = result_applies(t, info, n);
                if let Some(&first) = applies.first() {
                    owners.push((s, first, applies.iter().map(|a| a.apply).collect::<Vec<_>>()));
                }
            }
        }
        let [(s, first, ref applies)] = owners[..] else { return Err(RESULT_APPLY.into()) };

        let poly = t.u.poly(first.info).map(|p| p.binder);
        let mut generic = false;
        t.u.map_type(first.value, &mut |u, part| {
            generic |= poly.is_some_and(|b| u.param_ref(part).is_some_and(|(pb, _)| pb == b));
            None
        });
        if generic {
            return Err(RESULT_APPLY.into());
        }
        Resolution { prefix: Some(first.value), ..self }.choose(t, applies)?;
        Ok(s)
    }
}
