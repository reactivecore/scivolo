// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! References to local symbols escaping their scope (dotty's `escapingRefs`).

use super::type_ops::TypeOps;
use tasty::{SymbolId, Type, TypeId, TypeVars};

impl TypeOps<'_> {
    /// Checks whether type `t` mentions one of the definitions `syms` anywhere, also as the prefix
    /// of a selection or as the class of a `classOf` constant.
    pub fn refers_to(&mut self, t: TypeId, syms: &[SymbolId]) -> bool {
        let mut found = false;
        let mut classes = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            match *u.ty(part) {
                Type::Sym(_, s) => found |= syms.contains(&s),
                Type::Constant(tasty::tags::CLASSconst, bits) => classes.push(TypeId(bits as u32)),
                _ => {}
            }
            None
        });
        found || classes.into_iter().any(|c| self.refers_to(c, syms))
    }

    /// Checks whether type `t` mentions one of the definitions `locals` that go out of scope, as
    /// dotty looks for them: a member of an object reached by its full path does not count its
    /// prefix, the class of a `classOf` constant does count (dotty's `namedPartsWith`).
    pub fn escapes(&mut self, t: TypeId, locals: &[SymbolId]) -> bool {
        let mut found = false;
        let mut classes = Vec::new();
        self.u.map_type(t, &mut |u, part| match *u.ty(part) {
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

    /// Checks whether a type still to be inferred in `t` is bounded by a type mentioning one of
    /// the definitions `locals` that go out of scope, even after those bounds are approximated
    /// without them; approximating narrows the bounds in the constraint.
    pub fn var_bounds_escape(&mut self, t: TypeId, locals: &[SymbolId]) -> bool {
        self.var_bounds_refer_to(t, locals)
            && (!self.constraint.avoid_in_bounds(self.u, t) || self.var_bounds_refer_to(t, locals))
    }

    /// Checks whether a type still to be inferred in `t`, introduced outside the scope of the
    /// definitions `locals`, is bounded by a type mentioning one of them, directly or through the
    /// bounds of other types still to be inferred.
    pub fn var_bounds_refer_to(&mut self, t: TypeId, locals: &[SymbolId]) -> bool {
        self.bounds_refer_to(t, locals, false)
    }

    /// Checks whether a type still to be inferred in `t`, wherever introduced, is bounded by a
    /// type mentioning one of `locals`, also as the type of a value it is bounded by.
    pub fn any_var_bounds_refer_to(&mut self, t: TypeId, locals: &[SymbolId]) -> bool {
        self.bounds_refer_to(t, locals, true)
    }

    /// `var_bounds_refer_to`, for the types introduced inside the scope too and through values'
    /// types if `all`.
    fn bounds_refer_to(&mut self, t: TypeId, locals: &[SymbolId], all: bool) -> bool {
        if locals.is_empty() {
            return false;
        }
        let mut seen = Vec::new();
        let mut todo = vec![t];
        while let Some(t) = todo.pop() {
            let mut vars = Vec::new();
            self.u.map_type(t, &mut |u, part| {
                if let Some(v) = u.var(part) {
                    vars.push(v);
                }
                None
            });
            for v in vars {
                if seen.contains(&v) || self.constraint.instance(v).is_some() {
                    continue;
                }
                seen.push(v);
                let inner = !all && locals.iter().all(|&s| self.constraint.made_after(v, s));
                let (lo, hi) = self.constraint.bounds(v);
                let (lo, hi) = (self.zonk(lo), self.zonk(hi));
                if !inner && (self.refers_to(lo, locals) || self.refers_to(hi, locals)) {
                    return true;
                }
                if all {
                    let (wlo, whi) = (self.u.widen(lo), self.u.widen(hi));
                    if self.refers_to(wlo, locals) || self.refers_to(whi, locals) {
                        return true;
                    }
                }
                todo.extend([lo, hi]);
            }
        }
        false
    }
}
