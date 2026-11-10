// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//! Ranking the views of a constant tree that take it only widened, as `Int` widened to `Long`.

use super::super::super::Typer;
use super::super::super::widening::widens;
use super::super::candidates::Candidate;
use super::view_kinds::{ViewKind, view_kind};
use tasty::Binding;

/// Tells whether dotty ranks view `c` above each of `widened`, the views that take the constant
/// tree only widened: a conversion value above methods; a method if the value class of its
/// parameter widens to theirs and their owners do not derive from its owner.
pub fn beats_widened(t: &mut Typer<'_>, c: Candidate, widened: &[Candidate]) -> bool {
    if view_kind(t, c) == ViewKind::Value {
        return widened.iter().all(|&w| view_kind(t, w) == ViewKind::Method);
    }
    let Some(p) = param_class(t, c) else { return widened.is_empty() };
    let owner = t.u.symbol(c.sym).owner;
    widened.iter().all(|&w| {
        let other = t.u.symbol(w.sym).owner;
        let deeper = matches!((owner, other), (Some(o), Some(x)) if o != x && t.u.derives_from(x, o));
        !deeper && param_class(t, w).is_some_and(|q| widens(&p, &q))
    })
}

/// Picks the view of `widened` that dotty ranks above the others.
pub fn best_widened(t: &mut Typer<'_>, widened: &[Candidate]) -> Option<Candidate> {
    widened.iter().copied().find(|&w| {
        let others: Vec<Candidate> = widened.iter().copied().filter(|&o| o != w).collect();
        beats_widened(t, w, &others)
    })
}

/// The value class of the parameter of view method `c`.
fn param_class(t: &mut Typer<'_>, c: Candidate) -> Option<String> {
    let tpe = t.ref_type(c.prefix, c.sym);
    let info = t.method_info(tpe)?;
    let Some(Binding { params, implicit: false, .. }) = t.u.method(info) else {
        return None;
    };
    let [(_, p)] = params[..] else { return None };
    t.value_class(p)
}
