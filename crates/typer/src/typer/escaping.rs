//! References to local symbols escaping their scope (dotty's `escapingRefs`).

use super::Typer;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Whether type `t` refers to one of `locals` as dotty's `namedPartsWith` finds them: the
    /// prefix of a static member not traversed.
    pub fn escapes(&mut self, t: TypeId, locals: &[SymbolId]) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            let Type::Sym(pre, s) = *u.types().get(part) else { return None };
            found |= locals.contains(&s);
            pre.filter(|_| u.is_static_symbol(s)).map(|_| part)
        });
        found
    }
}
