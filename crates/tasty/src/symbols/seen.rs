// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Member types as seen from a prefix (dotty's `asSeenFrom`).

use super::base_types::MAX_DEPTH;
use super::{Kind, SymbolId, Universe};
use crate::tags::{INLINE, MUTABLE};
use crate::types::{Type, TypeId};

impl Universe {
    /// `t`, a type in class `owner`, as seen from `prefix`, from its `this` for a `super`: the
    /// `this` of `owner`, of its base classes and of its enclosing classes replaced by `prefix`
    /// or the prefix `prefix` gives them, the type parameters of `owner` by the arguments
    /// `prefix` gives them, one a wildcard by the parameter selected on a value path `prefix`
    /// (dotty's `argForParam`).
    pub fn as_seen_from(&mut self, t: TypeId, prefix: TypeId, owner: SymbolId) -> TypeId {
        let prefix = match *self.types().get(prefix) {
            Type::Super(this, _) => this,
            _ => prefix,
        };
        self.map_type(t, &mut |u, x| match *u.types().get(x) {
            Type::Sym(Some(_), s) if u.is_static_symbol(s) && !u.sees_opaques(s) => Some(x),
            Type::This(c) => {
                let c = u.class_symbol(c)?;
                u.this_seen_from(prefix, owner, c, MAX_DEPTH)
            }
            Type::Sym(Some(p), _) | Type::TypeRef(p, _) if !matches!(u.types().get(p), Type::This(_)) => None,
            _ => {
                let (c, i) = u.class_param(x)?;
                let prefix = match u.derives_from(owner, c) {
                    true => prefix,
                    false => u.this_seen_from(prefix, owner, c, MAX_DEPTH)?,
                };
                let seen = u.base_type(prefix, c)?;
                let arg = *u.applied(seen)?.1.get(i)?;
                if !matches!(u.types().get(arg), Type::Bounds(..)) || !u.is_value_path(prefix) {
                    return Some(arg);
                }
                let param = u.type_params(c)[i];
                Some(u.intern_type(Type::Sym(Some(prefix), param)))
            }
        })
    }

    /// Whether `t` is a stable path to a value: a skolem, or a reference to an immutable value or
    /// a parameter neither by-name nor inline, through `this`, a package, a type or such a path.
    pub fn is_value_path(&mut self, t: TypeId) -> bool {
        let (pre, s) = match *self.types().get(t) {
            Type::Skolem(..) => return true,
            Type::Sym(pre, s) => (pre, s),
            _ => return false,
        };
        let stable = match self.symbol(s).kind {
            Kind::Val => !self.symbol(s).flags.has(MUTABLE),
            Kind::Param => {
                let info = self.info(s);
                !self.symbol(s).flags.has(INLINE) && !matches!(self.types().get(info), Type::ByName(_))
            }
            _ => false,
        };
        stable
            && pre.is_none_or(|p| match *self.types().get(p) {
                Type::This(_) | Type::NoPrefix | Type::Package(_) | Type::PackageName(_) => true,
                Type::Sym(_, c) if !self.symbol(c).kind.is_term() => true,
                _ => self.is_value_path(p),
            })
    }

    /// What `this_cls.this` in class `cls` is as seen from `pre`: `pre` if it is an instance of
    /// `this_cls`, else what it is as seen from the prefix of `pre`'s base type `cls`, in the
    /// class enclosing `cls`; `None` to keep it.
    fn this_seen_from(
        &mut self,
        pre: TypeId,
        cls: SymbolId,
        this_cls: SymbolId,
        depth: u32,
    ) -> Option<TypeId> {
        let depth = depth.checked_sub(1)?;
        let pre_kind = self.types().get(pre).clone();
        if self.symbol(cls).kind == Kind::Package || matches!(pre_kind, Type::NoPrefix) {
            return None;
        }
        let widened = self.widen(pre);
        if self.derives_from(this_cls, cls) && self.base_type(widened, this_cls).is_some() {
            return Some(pre);
        }
        if matches!(pre_kind, Type::Package(_) | Type::PackageName(_)) {
            return None;
        }
        let base = self.base_type(widened, cls)?;
        let outer = self.normalized_prefix(base)?;
        let owner = self.symbol(cls).owner?;
        self.this_seen_from(outer, owner, this_cls, depth)
    }

    /// The prefix of class type `t`, through applications.
    fn normalized_prefix(&mut self, t: TypeId) -> Option<TypeId> {
        match *self.types().get(t) {
            Type::Sym(p, _) => p,
            Type::TypeRef(p, _) | Type::TermRef(p, _) => Some(p),
            Type::Applied(tycon, _) => self.normalized_prefix(tycon),
            _ => None,
        }
    }

    /// The type of member `m` of `prefix`; the `apply` of a dependent function type the one its
    /// refinement gives.
    pub fn member_info(&mut self, prefix: TypeId, m: SymbolId) -> TypeId {
        let info = self.info(m);
        match self.symbol(m).owner {
            Some(owner) if self.symbol(owner).kind == Kind::Class => {
                if let Some(refined) = self.dependent_refinement(prefix, m) {
                    return refined;
                }
                self.as_seen_from(info, prefix, owner)
            }
            _ => info,
        }
    }
}
