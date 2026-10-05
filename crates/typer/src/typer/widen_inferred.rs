//! The instance of a type variable from its lower bound (dotty's `widenInferred`): singletons
//! widened unless singleton-bounded, module classes as their objects.

use super::Typer;
use tasty::{Kind, Type, TypeId};

impl Typer<'_> {
    pub fn is_singleton(&mut self, t: TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::Constant(..) | Type::Str(_) | Type::This(_) | Type::TermRef(..) => true,
            Type::Sym(_, s) => !self.is_type_symbol(s),
            _ => false,
        }
    }

    /// `t` with its singletons widened, also inside a union, whose parts then join if one
    /// contains the others (dotty's `widenSingletons`).
    pub fn widen_singleton(&mut self, t: TypeId) -> TypeId {
        match self.u.types().get(t).clone() {
            Type::Or(a, b) => {
                let (a, b) = (self.widen_singleton(a), self.widen_singleton(b));
                self.u.lub(a, b)
            }
            _ if self.is_singleton(t) => {
                let widened = self.u.widen(t);
                if self.is_singleton_bounded(widened) { t } else { widened }
            }
            _ => t,
        }
    }

    /// Whether `t` is an abstract type or type parameter with a singleton upper bound.
    fn is_singleton_bounded(&mut self, t: TypeId) -> bool {
        let Type::Sym(_, s) = *self.u.types().get(t) else { return false };
        if !matches!(self.u.symbol(s).kind, Kind::TypeParam | Kind::Type) {
            return false;
        }
        self.complete(s);
        let info = self.u.info(s);
        match *self.u.types().get(info) {
            Type::Bounds(_, hi) => self.is_singleton(hi),
            _ => false,
        }
    }

    /// The object whose module class `t` is, as a reference, or `t`.
    pub fn module_ref(&mut self, t: TypeId) -> TypeId {
        let Type::Sym(prefix, c) = *self.u.types().get(t) else { return t };
        let sym = self.u.symbol(c);
        if sym.kind != Kind::Class || !sym.flags.has(tasty::tags::OBJECT) {
            return t;
        }
        let (Some(owner), Some(name)) = (sym.owner, sym.name.strip_suffix('$').map(str::to_owned)) else {
            return t;
        };
        let module =
            self.u.decl(owner, &name).unwrap_or_default().into_iter().find(|&m| {
                self.u.symbol(m).kind == Kind::Val && self.u.symbol(m).flags.has(tasty::tags::OBJECT)
            });
        match module {
            Some(m) => self.u.intern_type(Type::Sym(prefix, m)),
            None => t,
        }
    }
}
