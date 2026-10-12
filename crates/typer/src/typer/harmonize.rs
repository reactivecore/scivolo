//! Where dotty's `harmonize` would convert numeric values to a common class, which the typer
//! refuses.

use super::Typer;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// Whether types `a` and `b` are different numeric classes, which dotty's `harmonize` widens.
    pub fn needs_harmonizing(&mut self, a: TypeId, b: TypeId) -> bool {
        let (ca, cb) = (self.value_class(a), self.value_class(b));
        let numeric = |c: &Option<String>| c.as_deref().is_some_and(super::widening::is_numeric);
        numeric(&ca) && numeric(&cb) && ca != cb
    }

    /// Whether dotty's `harmonize` converts the `Int` literals among values of `types` to the
    /// one other numeric class of the rest.
    pub fn harmonizes(&mut self, types: &[TypeId]) -> bool {
        let (mut class, mut int_literal) = (None, false);
        for &t in types {
            let widened = self.u.widen(t);
            let int = |x: TypeId| matches!(self.u.types().get(x), Type::Constant(tasty::tags::INTconst, _));
            if int(t) || int(widened) {
                int_literal = true;
                continue;
            }
            let c = self.value_class(t);
            if !c.as_deref().is_some_and(super::widening::is_numeric) || class.is_some() && class != c {
                return false;
            }
            class = c;
        }
        int_literal && class.is_some_and(|c| c != "scala.Int")
    }
}
