// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Expected types of implicit searches with type variables outside the subset (unbounded ones,
//! ones in unions), or too unspecific to search; variables settled by their bounds.

use super::super::Typer;
use tasty::{Type, TypeId, TypeVars};

impl Typer<'_> {
    /// Whether `t` has an uninstantiated type variable without bounds.
    pub fn has_unbounded_var(&mut self, t: TypeId) -> bool {
        let vars = self.open_vars(t);
        vars.into_iter().any(|v| self.constraint.bounds(v) == (tasty::NOTHING, tasty::ANY))
    }

    /// What about the type variables of expected type `t` is outside the subset: a union with
    /// one, or one applied as a type constructor.
    pub fn unsettled(&mut self, t: TypeId) -> Option<&'static str> {
        if self.has_open_union(t) {
            return Some("implicit search for a union with type variables");
        }
        let mut constructor = false;
        self.u.map_type(t, &mut |u, part| {
            constructor |=
                matches!(u.types().get(part), Type::Applied(c, _) if matches!(u.types().get(*c), Type::Var(_)));
            None
        });
        constructor.then_some("implicit search for a type with a higher-kinded type variable")
    }

    /// Whether `t` applies an uninstantiated type variable of id `from` or later whose bounds
    /// differ.
    pub fn has_open_constructor(&mut self, t: TypeId, from: usize) -> bool {
        let mut tycons = Vec::new();
        self.u.map_type(t, &mut |u, part| {
            if let Type::Applied(c, _) = *u.types().get(part) {
                tycons.push(c);
            }
            None
        });
        tycons.into_iter().any(|c| {
            let c = self.zonk(c);
            let Type::Var(v) = *self.u.types().get(c) else { return false };
            let (lo, hi) = self.constraint.bounds(v);
            v.0 as usize >= from && self.constraint.is_pending(v) && lo != hi
        })
    }

    /// The upper bound of `t` if `t` is an uninstantiated type variable bounded from above only,
    /// or one applied to arguments bounded by a class taking as many, applied to them, of an
    /// intersection with such a side, the sides so: what the search looks for in its place
    /// (dotty's `wildApprox`).
    pub fn var_upper_bound(&mut self, t: TypeId) -> Option<TypeId> {
        let t = self.zonk(t);
        let (var, args) = match self.u.types().get(t).clone() {
            Type::Var(v) => (v, None),
            Type::Applied(c, args) => match *self.u.types().get(c) {
                Type::Var(v) => (v, Some(args)),
                _ => return None,
            },
            Type::And(a, b) => {
                let (x, y) = (self.var_upper_bound(a), self.var_upper_bound(b));
                if x.is_none() && y.is_none() {
                    return None;
                }
                return Some(self.u.intern_type(Type::And(x.unwrap_or(a), y.unwrap_or(b))));
            }
            _ => return None,
        };
        let (lo, hi) = self.constraint.bounds(var);
        if lo != tasty::NOTHING || hi == tasty::ANY {
            return None;
        }
        let hi = self.zonk(hi);
        let Some(args) = args else { return Some(hi) };
        let class = self.u.class_symbol(hi).filter(|_| matches!(self.u.types().get(hi), Type::Sym(..)))?;
        (self.u.type_params(class).len() == args.len()).then(|| self.u.intern_type(Type::Applied(hi, args)))
    }

    /// `t` with its type variables whose bounds are equal instantiated to them.
    pub fn determined(&mut self, t: TypeId) -> TypeId {
        let t = self.zonk(t);
        for v in self.open_vars(t) {
            let (lo, hi) = self.constraint.bounds(v);
            if lo == hi {
                self.constraint.instantiate(v, lo);
            }
        }
        self.zonk(t)
    }

    /// Whether `t` is `Any`, `AnyRef`, or an uninstantiated type variable bounded by them, which no
    /// search may answer (dotty's `isUnderspecified`).
    pub fn is_underspecified(&mut self, t: TypeId) -> bool {
        let t = self.zonk(t);
        match self.u.types().get(t).clone() {
            Type::Var(v) => {
                let (_, hi) = self.constraint.bounds(v);
                self.is_underspecified(hi)
            }
            Type::Or(a, b, _) if self.u.class_symbol(b) == Some(self.u.builtins().null) => {
                self.is_underspecified(a)
            }
            Type::Sym(..) | Type::TypeRef(..) => {
                let b = self.u.builtins();
                let t = self.u.dealias(t);
                let class = match self.u.types().get(t).clone() {
                    Type::Sym(_, s) => Some(s),
                    Type::TypeRef(p, n) => self.u.resolve(p, &n, true),
                    _ => None,
                };
                class.is_some_and(|c| c == b.any || c == b.object)
            }
            _ => false,
        }
    }

    /// Whether `t`, its aliases expanded, has a union with an uninstantiated type variable in it.
    pub fn has_open_union(&mut self, t: TypeId) -> bool {
        self.open_union_within(t, 16)
    }

    fn open_union_within(&mut self, t: TypeId, depth: u32) -> bool {
        let Some(depth) = depth.checked_sub(1) else { return false };
        let t = self.u.dealias(t);
        match self.u.types().get(t).clone() {
            Type::Or(..) => !self.open_vars(t).is_empty(),
            Type::And(a, b) => self.open_union_within(a, depth) || self.open_union_within(b, depth),
            Type::Applied(_, args) => args.into_iter().any(|a| self.open_union_within(a, depth)),
            Type::Annotated(a, _) | Type::ByName(a) => self.open_union_within(a, depth),
            Type::Bounds(lo, hi) => self.open_union_within(lo, depth) || self.open_union_within(hi, depth),
            _ => false,
        }
    }
}
