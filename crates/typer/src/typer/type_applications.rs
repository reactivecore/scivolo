// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeApplications.scala

use super::Typer;
use tasty::tags::OPAQUE;
use tasty::{Binding, Kind, Type, TypeId};

impl Typer<'_> {
    /// `tycon[args]`; an alias that eta-expands a type constructor, `type F[X] = G[X]`, applied
    /// as that constructor, also through aliases of it as `type H = F` (dotty's `appliedTo`).
    pub fn applied_to(&mut self, tycon: TypeId, args: Vec<TypeId>) -> TypeId {
        if let Some(alias) = self.dealiased_lambda(tycon)
            && let Some(Binding { binder, params, result, .. }) = self.u.lambda(alias)
            && let Some((inner, inner_args)) = self.u.applied(result)
            && inner_args.len() == params.len()
            && args.len() == params.len()
            && inner_args.iter().enumerate().all(|(i, &a)| match *self.u.ty(a) {
                Type::Param(b, k) => b == binder && k == i as u32,
                Type::Local(b, name) => b.0 == binder.0 && name == params[i].0,
                _ => false,
            })
        {
            return self.u.intern_applied(inner, &args);
        }
        self.u.intern_applied(tycon, &args)
    }

    /// The type lambda that type member reference `tycon` is an alias of, through aliases of
    /// other type members, as `type H = F` for `type F[X] = G[X]`; an alias to a member is seen
    /// from the prefix of its reference, a type lambda as declared; an opaque type ends the chain
    /// (dotty's `safeDealias`).
    fn dealiased_lambda(&mut self, tycon: TypeId) -> Option<TypeId> {
        let mut t = tycon;
        for _ in 0..tasty::MAX_STEPS {
            let s = self.u.sym_of(t).filter(|&s| self.u.symbol(s).kind == Kind::Type)?;
            if self.u.symbol(s).flags.has(OPAQUE) {
                return None;
            }
            self.complete(s);
            if let Some(declared) = self.u.alias_of(s).filter(|&a| self.u.lambda(a).is_some()) {
                return Some(declared);
            }
            t = self.u.referenced_alias(t)?;
            if self.u.lambda(t).is_some() {
                return Some(t);
            }
        }
        None
    }
}
