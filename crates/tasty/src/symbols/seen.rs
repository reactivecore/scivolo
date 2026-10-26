// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Member types as seen from a prefix (dotty's `asSeenFrom`).

use super::base_types::MAX_DEPTH;
use super::{Kind, SymbolId, Universe};
use crate::types::{Type, TypeId};

impl Universe {
    /// `t`, a type in class `owner`, as seen from `prefix`, from its `this` for a `super`: the
    /// `this` of `owner`, of its base classes and of its enclosing classes replaced by `prefix`
    /// or the prefix `prefix` gives them, the type parameters of `owner` by the arguments
    /// `prefix` gives them.
    pub fn as_seen_from(&mut self, t: TypeId, prefix: TypeId, owner: SymbolId) -> TypeId {
        self.seen_from(t, prefix, owner, false)
    }

    /// `as_seen_from`, a type parameter whose argument is a wildcard selected from `prefix` if
    /// `prefix` is a value, as dotty's `argForParam` does: for aliases selected from a path and
    /// the parameters of methods, whose types the typer need not avoid.
    pub fn seen_from_path(&mut self, t: TypeId, prefix: TypeId, owner: SymbolId) -> TypeId {
        self.seen_from(t, prefix, owner, true)
    }

    fn seen_from(&mut self, t: TypeId, prefix: TypeId, owner: SymbolId, wildcards: bool) -> TypeId {
        let prefix = match *self.types().get(prefix) {
            Type::Super(this, _) => this,
            _ => prefix,
        };
        self.map_type(t, &mut |u, x| match u.types().get(x).clone() {
            Type::Sym(Some(_), s) if u.is_static_symbol(s) && !u.sees_opaques(s) => Some(x),
            Type::This(c) => {
                let c = u.class_symbol(c)?;
                u.this_seen_from(prefix, owner, c, MAX_DEPTH)
            }
            _ => {
                let (c, i) = u.class_param(x)?;
                let prefix = match u.derives_from(owner, c) {
                    true => prefix,
                    false => u.this_seen_from(prefix, owner, c, MAX_DEPTH)?,
                };
                let seen = u.base_type(prefix, c)?;
                let arg = match u.types().get(seen) {
                    Type::Applied(_, args) => args.get(i).copied()?,
                    _ => return None,
                };
                match wildcards {
                    true => Some(u.wildcard_seen(x, arg, prefix).unwrap_or(arg)),
                    false => Some(arg),
                }
            }
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

    /// Class type parameter `param`, of a `this` or no prefix, selected from value `prefix` when
    /// the argument `prefix`'s type gives it is the wildcard `arg` (dotty's `argForParam`).
    fn wildcard_seen(&mut self, param: TypeId, arg: TypeId, prefix: TypeId) -> Option<TypeId> {
        let Type::Sym(pre, p) = *self.types().get(param) else { return None };
        let this = pre.is_none_or(|pre| matches!(self.types().get(pre), Type::This(_)));
        let value = match *self.types().get(prefix) {
            Type::TermRef(..) => true,
            Type::Sym(_, s) => matches!(self.symbol(s).kind, Kind::Val | Kind::Param),
            _ => false,
        };
        let wildcard = matches!(self.types().get(arg), Type::Bounds(..));
        (this && value && wildcard).then(|| self.intern_type(Type::Sym(Some(prefix), p)))
    }

    /// The prefix of class type `t`, through applications.
    fn normalized_prefix(&mut self, t: TypeId) -> Option<TypeId> {
        match self.types().get(t).clone() {
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
                let seen = self.as_seen_from(info, prefix, owner);
                self.with_params_seen_from_path(info, seen, prefix, owner)
            }
            _ => info,
        }
    }

    /// Method type `seen`, `info` seen from `prefix`, with the types of its parameters
    /// `seen_from_path`.
    fn with_params_seen_from_path(
        &mut self,
        info: TypeId,
        seen: TypeId,
        prefix: TypeId,
        owner: SymbolId,
    ) -> TypeId {
        let (Type::Method { params: own, .. }, Type::Method { binder, params, result, implicit, contextual }) =
            (self.types().get(info).clone(), self.types().get(seen).clone())
        else {
            return seen;
        };
        let mut changed = false;
        let params: Vec<(String, TypeId)> = own
            .into_iter()
            .zip(params)
            .map(|((_, p), (name, q))| {
                let r = self.seen_from_path(p, prefix, owner);
                changed |= r != q;
                (name, r)
            })
            .collect();
        match changed {
            true => self.intern_type(Type::Method { binder, params, result, implicit, contextual }),
            false => seen,
        }
    }
}
