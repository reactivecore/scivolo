// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//! The type of an inherited member as seen from `this` of a class, its references to type
//! members denoting the members of that class (dotty's `asSeenFrom` of named types).

use super::Typer;
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    /// Determines the type of member `m` as seen from `this` of class `cls`, a reference to a
    /// type member `T` of `this` denoting the `T` that `cls` has.
    pub fn seen_from_class(&mut self, m: SymbolId, cls: SymbolId) -> TypeId {
        let class = crate::defn::type_ref(self.u, cls);
        let this = self.u.intern_type(Type::This(class));
        let info = self.u.info(m);
        let seen = match self.u.symbol(m).owner {
            Some(owner) => self.u.as_seen_from(info, this, owner),
            None => info,
        };
        let mut rebound = Vec::new();
        self.u.map_type(seen, &mut |u, x| {
            if let Type::Sym(Some(p), s) = *u.ty(x)
                && p == this
                && u.is_type_symbol(s)
                && u.symbol(s).owner != Some(cls)
            {
                rebound.push((x, p, s));
            }
            None
        });
        let mut out = seen;
        for (x, p, s) in rebound {
            let name = self.u.symbol(s).name.clone();
            let own = self.class_members(cls, &name).into_iter().find(|&o| self.u.is_type_symbol(o));
            if let Some(o) = own.filter(|&o| o != s) {
                let to = self.u.intern_type(Type::Sym(Some(p), o));
                out = self.u.map_type(out, &mut |_, y| (y == x).then_some(to));
            }
        }
        out
    }
}
