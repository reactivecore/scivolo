// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeApplications.scala
//! The arities of type constructors (dotty's `typeParams`, `hkTypeParams`) and the bounds of
//! type parameters of any kind.

use super::Typer;
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    /// How many type parameters type constructor `t` takes: a class's, a type lambda's, a type
    /// member's as the refinements of its prefix give it.
    pub fn type_arity(&mut self, t: TypeId) -> usize {
        self.type_arity_within(t, 32)
    }

    fn type_arity_within(&mut self, t: TypeId, depth: u32) -> usize {
        let Some(depth) = depth.checked_sub(1) else { return 0 };
        match self.u.types().get(t).clone() {
            Type::Sym(pre, s) => match self.u.symbol(s).kind {
                Kind::Class => self.u.type_params(s).len(),
                Kind::Type | Kind::TypeParam => {
                    self.complete(s);
                    let info = self.refined_info(pre, s);
                    self.info_arity(info, depth)
                }
                _ => 0,
            },
            Type::Lambda { params, .. } => params.len(),
            Type::TypeRef(pre, name) => match self.u.refinement_named(pre, &name) {
                Some(info) => self.info_arity(info, depth),
                None => self.u.resolve_ref(t).map_or(0, |s| {
                    let sym = self.u.intern_type(Type::Sym(Some(pre), s));
                    self.type_arity_within(sym, depth)
                }),
            },
            Type::Applied(tycon, _) => {
                if let Type::Sym(_, s) = *self.u.types().get(tycon) {
                    self.complete(s);
                }
                let reduced = self.u.dealias(t);
                if reduced == t { 0 } else { self.type_arity_within(reduced, depth) }
            }
            _ => 0,
        }
    }

    /// How many type parameters a type member of info `info` takes.
    fn info_arity(&mut self, info: TypeId, depth: u32) -> usize {
        match self.u.types().get(info).clone() {
            Type::Alias(a) | Type::Bounds(_, a) => self.type_arity_within(a, depth),
            Type::Lambda { params, .. } => params.len(),
            _ => 0,
        }
    }

    /// The lower and upper bound of type parameter info `bounds` and the arity of the parameter;
    /// of a higher-kinded one, `[X] =>> >: L <: H`, `[X] =>> L` and `[X] =>> H`, the lower one
    /// `None` for `Nothing`.
    pub fn bounds_of(&mut self, bounds: TypeId) -> (Option<TypeId>, TypeId, usize) {
        match self.u.types().get(bounds).clone() {
            Type::Bounds(lo, hi) => (Some(lo), hi, self.type_arity(hi)),
            Type::Lambda { binder, params, result } => {
                let (lo, hi) = match *self.u.types().get(result) {
                    Type::Bounds(lo, hi) => (lo, hi),
                    _ => (result, result),
                };
                let bottom = self.u.class_symbol(lo) == Some(self.u.builtins().nothing);
                let arity = params.len();
                let mut lambda = |result| {
                    let params = params.clone();
                    self.u.intern_type(Type::Lambda { binder, params, result })
                };
                let lo = lambda(lo);
                ((!bottom).then_some(lo), lambda(hi), arity)
            }
            _ => (Some(bounds), bounds, self.type_arity(bounds)),
        }
    }
}
