// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeApplications.scala

use super::Typer;
use tasty::tags::OPAQUE;
use tasty::{Binding, Kind, Type, TypeId};

impl Typer<'_> {
    /// `tycon[args]`; an alias that eta-expands a type constructor, `type F[X] = G[X]`, applied
    /// as that constructor, also through aliases of it as `type H = F` and when `G` is such an
    /// alias in turn (dotty's `appliedTo`).
    pub fn applied_to(&mut self, tycon: TypeId, args: Vec<TypeId>) -> TypeId {
        let mut tycon = tycon;
        for _ in 0..tasty::MAX_STEPS {
            match self.eta_reduced(tycon, args.len()) {
                Some(inner) => tycon = inner,
                None => break,
            }
        }
        self.u.intern_applied(tycon, &args)
    }

    /// `G` if `tycon` is an alias of type lambda `[X1, ..., Xn] =>> G[X1, ..., Xn]` with `arity`
    /// parameters.
    fn eta_reduced(&mut self, tycon: TypeId, arity: usize) -> Option<TypeId> {
        let alias = self.dealiased_lambda(tycon)?;
        let Binding { binder, params, result, .. } = self.u.lambda(alias)?;
        let (inner, inner_args) = self.u.applied(result)?;
        let eta = inner_args.len() == params.len()
            && arity == params.len()
            && inner_args.iter().enumerate().all(|(i, &a)| match *self.u.ty(a) {
                Type::Param(b, k) => b == binder && k == i as u32,
                Type::Local(b, name) => b.0 == binder.0 && name == params[i].0,
                _ => false,
            });
        eta.then_some(inner)
    }

    /// The type lambda that type member reference `tycon` is an alias of, through aliases of
    /// other type members, as `type H = F` for `type F[X] = G[X]`, each seen from the prefix of
    /// its reference; an opaque type ends the chain (dotty's `safeDealias`).
    fn dealiased_lambda(&mut self, tycon: TypeId) -> Option<TypeId> {
        let mut t = tycon;
        for _ in 0..tasty::MAX_STEPS {
            let s = self.u.sym_of(t).filter(|&s| self.u.symbol(s).kind == Kind::Type)?;
            if self.u.symbol(s).flags.has(OPAQUE) {
                return None;
            }
            self.complete(s);
            t = self.u.referenced_alias(t)?;
            if self.u.lambda(t).is_some() {
                return Some(t);
            }
        }
        None
    }
}
