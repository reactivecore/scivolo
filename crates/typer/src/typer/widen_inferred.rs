//! The instance of a type variable from its lower bound (dotty's `widenInferred`): singletons
//! widened unless singleton-bounded, module classes as their objects.

use super::Typer;
use tasty::{Kind, Type, TypeId, TypeVars};

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
                let widened = match *self.u.types().get(widened) {
                    Type::ByName(r) => r,
                    _ => widened,
                };
                if self.is_singleton_bounded(widened) { t } else { widened }
            }
            _ => t,
        }
    }

    /// `t` without transparent traits among its intersection parts, one at a time while the
    /// result conforms to `bound`; `t` if each dropped trait contains the result (dotty's
    /// `dropTransparentTraits`).
    pub fn without_transparent_traits(&mut self, t: TypeId, bound: TypeId) -> TypeId {
        let mark = self.constraint.mark();
        let (mut kept, mut dropped) = (Vec::new(), Vec::new());
        let mut current = t;
        for _ in 0..32 {
            let narrower = self.drop_one_transparent(current, &kept, &mut dropped);
            if narrower == current {
                break;
            }
            if self.conforms(narrower, bound) {
                current = narrower;
            } else {
                kept.extend(dropped.pop());
            }
        }
        if current == t || dropped.iter().all(|&d| self.u.is_subtype(current, d)) {
            self.constraint.rollback(mark);
            return t;
        }
        current
    }

    /// `t` with its first transparent trait not in `kept` replaced by `Any`, pushed to `dropped`.
    fn drop_one_transparent(&mut self, t: TypeId, kept: &[TypeId], dropped: &mut Vec<TypeId>) -> TypeId {
        if self.is_transparent_trait(t) && !kept.contains(&t) {
            dropped.push(t);
            return tasty::ANY;
        }
        let Type::And(a, b) = *self.u.types().get(t) else { return t };
        let a2 = self.drop_one_transparent(a, kept, dropped);
        if a2 != a {
            return self.u.glb(a2, b);
        }
        let b2 = self.drop_one_transparent(b, kept, dropped);
        if b2 != b { self.u.glb(a, b2) } else { t }
    }

    /// Whether `t` is a transparent trait (dotty's `isTransparent(traitOnly = true)`).
    fn is_transparent_trait(&mut self, t: TypeId) -> bool {
        let tycon = match *self.u.types().get(t) {
            Type::Applied(c, _) => c,
            _ => t,
        };
        let Type::Sym(_, c) = *self.u.types().get(tycon) else { return false };
        let s = self.u.symbol(c);
        let assumed = matches!(self.u.full_name(c).as_str(), "java.lang.Comparable" | "java.io.Serializable");
        s.kind == Kind::Class
            && s.flags.has(tasty::tags::TRAIT)
            && (s.flags.has(tasty::tags::TRANSPARENT) || assumed)
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
