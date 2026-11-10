// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Selectors of pattern definitions and generators, ascribed `@RuntimeChecked`, and their
//! refutable patterns (dotty's `typedMatch` rewrite, `Checking.checkIrrefutable`).

mod check;
mod warnings;

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Type, TypeId, Universe};

/// The annotation `(x: @unchecked)` that turns off the checks of a pattern's selector.
pub const UNCHECKED: &str = "scala.unchecked";
/// The annotation `x.runtimeChecked` gives a selector, matched at run time without a warning.
const RUNTIME_CHECKED: &str = "scala.annotation.internal.RuntimeChecked";

/// Why a pattern may not match its selector.
pub enum Refutable {
    /// The pattern's type, of the pattern tree, is narrower than the selector's type.
    NonConforming(TNodeId, TypeId),
    /// The extractor pattern's extractor may not match.
    Extractor(TNodeId),
    /// The expected types of an extractor's patterns are unknown.
    Unknown,
}

impl Typer<'_> {
    /// Ascribes `@RuntimeChecked` to selector `sel` of a pattern definition (`pat_def`), as
    /// `val Some(x) = opt`, or of a generator whose pattern is `pat`, unless annotated so or
    /// `@unchecked`; warns about a refutable `pat` in a definition. Refuses a refutable pattern
    /// in a generator and an otherwise annotated selector.
    pub fn runtime_checked(
        &mut self,
        sel: TNodeId,
        pat: TNodeId,
        pat_def: bool,
    ) -> Result<TNodeId, &'static str> {
        let sel_tpe = self.tpd.tpe(sel);
        let sel_tpe = self.zonk(sel_tpe);
        if has_annotation(self.u, sel_tpe, UNCHECKED) || has_annotation(self.u, sel_tpe, RUNTIME_CHECKED) {
            return Ok(sel);
        }
        if matches!(self.u.types().get(sel_tpe), Type::Annotated(..)) {
            return Err("annotated selector of a pattern definition");
        }
        if let Err(reason) = check::check_irrefutable(self, pat, sel_tpe) {
            if !pat_def || matches!(reason, Refutable::Unknown) {
                return Err(if pat_def {
                    "refutable pattern definition"
                } else {
                    "refutable pattern in a generator"
                });
            }
            warnings::warn_refutable(self, reason, sel);
        }

        let annot =
            self.resolve_full_name(RUNTIME_CHECKED, true).ok_or("RuntimeChecked missing from the library")?;
        let annot = crate::defn::type_ref(self.u, annot);
        let widened = self.u.widen(sel_tpe);
        let tpe = self.u.intern_type(Type::Annotated(widened, annot));
        let span = self.tpd.span(sel);
        let tpt = self.type_tree(tpe, span.end_pos(), true);
        Ok(self.tpd.node(T::Typed { expr: sel, tpt }, span.to_synthetic(), Some(tpe), None))
    }
}

/// Whether `t` or a type it annotates carries annotation class `name`.
fn has_annotation(u: &mut Universe, t: TypeId, name: &str) -> bool {
    let Type::Annotated(inner, annot) = *u.types().get(t) else { return false };
    u.class_symbol(annot).is_some_and(|c| u.full_name(c) == name) || has_annotation(u, inner, name)
}
