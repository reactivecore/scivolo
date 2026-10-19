// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenExpr.scala
//! `finally` (nscplugin's `genTryFinally`): every control-flow edge leaving the instructions of
//! a `try` and its handlers first runs a copy of the finalizer.

use super::Gen;
use super::buf::Buf;
use crate::Result;
use nir::{Inst, Local, Next};
use rustc_hash::FxHashSet;

impl Gen<'_, '_> {
    /// The instructions `insts` with what `finalizer` generates on each edge leaving them, the
    /// finalizer copies after them.
    pub fn with_finally(
        &mut self,
        mut finalizer: impl FnMut(&mut Self) -> Result<()>,
        insts: Buf,
    ) -> Result<Buf> {
        let labels: FxHashSet<Local> = insts
            .insts
            .iter()
            .filter_map(|i| if let Inst::Label(l, _) = i { Some(*l) } else { None })
            .collect();
        let mut kept = Vec::new();
        let (result, finalizers) = self.nested(|g| -> Result<()> {
            for (inst, pos) in insts.insts.into_iter().zip(insts.positions) {
                if matches!(inst, Inst::Label(..) | Inst::Let { .. }) || is_internal(&inst, &labels) {
                    kept.push((inst, pos));
                    continue;
                }
                let finally_label = g.fresh();
                let saved = g.at(pos);
                let scope = g.enter_scope(pos);
                g.label(finally_label, Vec::new());
                finalizer(g)?;
                g.m.scope = scope;
                g.push(inst, pos);
                g.m.pos = saved;
                kept.push((Inst::Jump(Next::Label(finally_label, Vec::new())), pos));
            }
            Ok(())
        });
        result?;
        Ok(Buf::concat(kept, finalizers))
    }
}

/// Whether control flow `inst` stays within the instructions labelled `labels`.
fn is_internal(inst: &Inst, labels: &FxHashSet<Local>) -> bool {
    let inside = |n: &Next| target(n).is_some_and(|l| labels.contains(&l));
    match inst {
        Inst::Jump(n) | Inst::Throw(_, n) => inside(n),
        Inst::If(_, a, b) | Inst::LinktimeIf(_, a, b) => inside(a) && inside(b),
        Inst::Switch(_, d, cases) => inside(d) && cases.iter().all(inside),
        _ => false,
    }
}

/// The label `n` continues at (nir's `Next.id`).
fn target(n: &Next) -> Option<Local> {
    match n {
        Next::None => None,
        Next::Unwind(_, n) | Next::Case(_, n) => target(n),
        Next::Label(l, _) => Some(*l),
    }
}
