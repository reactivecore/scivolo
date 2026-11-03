// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeComparer.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! References to an opaque type through the path of its object, inside the object (where
//! dotty's `liftToThis` and `tryLiftToThis` apply).

use super::Typer;
use crate::ctx::Ctx;
use tasty::{Type, TypeId};

pub const OPAQUE_THROUGH_OBJECT: &str = "opaque type through its object's path inside the object";

impl Typer<'_> {
    /// Whether `t`, widened, refers to an opaque type of the script through the path of an
    /// enclosing object, and its right-hand side has a member `name`.
    pub fn opaque_member_through_object(&mut self, t: TypeId, name: &str, ctx: Ctx) -> bool {
        let Some(alias) = self.opaque_through_object(t) else { return false };
        !self.type_members_in(alias, name, ctx).is_empty()
    }

    /// Whether `t`, widened, refers to an opaque type of the script through the path of an
    /// enclosing object.
    pub fn is_opaque_through_object(&mut self, t: TypeId) -> bool {
        self.opaque_through_object(t).is_some()
    }

    /// The right-hand side of the opaque type `t`, widened, refers to through the path of an
    /// enclosing object.
    fn opaque_through_object(&mut self, t: TypeId) -> Option<TypeId> {
        let w = self.u.widen(t);
        let mut d = self.u.dealias(w);
        if let Type::Applied(tycon, _) = *self.u.types().get(d) {
            d = tycon;
        }
        let (prefix, s) = match *self.u.types().get(d) {
            Type::Sym(Some(p), s) => (p, Some(s)),
            Type::TypeRef(p, _) => (p, self.u.resolve_ref(d)),
            _ => return None,
        };
        let alias = self.u.opaque_alias(s?)?;
        let module = match *self.u.types().get(prefix) {
            Type::Sym(_, m) if self.u.symbol(m).kind.is_term() => Some(m),
            Type::TermRef(..) => self.u.resolve_ref(prefix),
            _ => None,
        };
        self.u.enclosing_opaque_module(module?)?;
        Some(alias)
    }
}
