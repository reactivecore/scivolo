// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! References to local symbols escaping their scope (dotty's `escapingRefs`).

use super::Typer;
use tasty::{SymbolId, Type, TypeId, TypeVars};

impl Typer<'_> {
    /// Whether type `t` refers to one of `locals` as dotty's `namedPartsWith` finds them: the
    /// prefix of a static member not traversed, the class of a class constant traversed.
    pub fn escapes(&mut self, t: TypeId, locals: &[SymbolId]) -> bool {
        let mut found = false;
        let mut classes = Vec::new();
        self.u.map_type(t, &mut |u, part| match *u.types().get(part) {
            Type::Sym(pre, s) => {
                found |= locals.contains(&s);
                pre.filter(|_| u.is_static_symbol(s)).map(|_| part)
            }
            Type::Constant(tasty::tags::CLASSconst, bits) => {
                classes.push(TypeId(bits as u32));
                None
            }
            _ => None,
        });
        found || classes.into_iter().any(|c| self.escapes(c, locals))
    }

    /// Whether an uninstantiated type variable in `t` has a bound that refers to one of `locals`,
    /// directly or through the bounds of the variables in it.
    pub fn var_bounds_refer_to(&mut self, t: TypeId, locals: &[SymbolId]) -> bool {
        if locals.is_empty() {
            return false;
        }
        let mut seen = Vec::new();
        let mut todo = vec![t];
        while let Some(t) = todo.pop() {
            let mut vars = Vec::new();
            self.u.map_type(t, &mut |u, part| {
                if let Type::Var(v) = *u.types().get(part) {
                    vars.push(v);
                }
                None
            });
            for v in vars {
                if seen.contains(&v) || self.constraint.instance(v).is_some() {
                    continue;
                }
                seen.push(v);
                let (lo, hi) = self.constraint.bounds(v);
                let (lo, hi) = (self.zonk(lo), self.zonk(hi));
                if self.refers_to(lo, locals) || self.refers_to(hi, locals) {
                    return true;
                }
                todo.extend([lo, hi]);
            }
        }
        false
    }
}
