// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/TypeTestsCasts.scala
//! The unchecked warning of type patterns (dotty's `TypeTestsCasts.checkTypePattern`).

use super::irrefutable::UNCHECKED;
use super::{Pt, Typer};
use syntax::ast::Span;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// dotty's unchecked warning at `span` about testing a value of type `pt` for `tpe` that
    /// refers to an abstract type or is a refinement, unless `pt` conforms or is `@unchecked`.
    pub fn warn_unchecked_type_test(&mut self, pt: Pt, tpe: TypeId, span: Span) {
        let Pt::Type(p) = pt else { return };
        if self.without_annotation(p, UNCHECKED) != p {
            return;
        }
        let Some(why) = self.unchecked_reason(p, tpe) else { return };
        let shown = self.plain_text(tpe).unwrap_or_else(|| self.u.show_user(tpe));
        let text = format!("the type test for {shown} cannot be checked at runtime because {why}");
        self.erasure_diags.warning_at(span.start(), span.end(), span.point(), text);
    }

    /// Why a value of type `x` cannot be tested for `p` at runtime (dotty's `whyUncheckable`,
    /// its `recur` for abstract and refined types).
    fn unchecked_reason(&mut self, x: TypeId, p: TypeId) -> Option<&'static str> {
        if self.u.is_subtype(x, p) {
            return None;
        }
        let p = self.dealiased(p);
        match self.u.types().get(p).clone() {
            Type::And(a, b) | Type::Or(a, b, _) => {
                self.unchecked_reason(x, a).or_else(|| self.unchecked_reason(x, b))
            }
            Type::Annotated(t, _) | Type::Rec(_, t) => self.unchecked_reason(x, t),
            Type::Refined(parent, name, info) => self.unchecked_reason(x, parent).or_else(|| {
                let refined = self.u.intern_type(Type::Refined(x, name, info));
                (!self.u.is_subtype(x, refined)).then_some("it's a refinement type")
            }),
            _ if self.is_abstract_type(p) => Some("it refers to an abstract type member or type parameter"),
            _ => None,
        }
    }
}
