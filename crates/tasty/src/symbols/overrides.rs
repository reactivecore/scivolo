// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Denotations.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Whether one method's signature matches another's as members of the same class (dotty's
//! `matches`), on types in a normal form.

use super::{Kind, SymbolId, Universe};
use crate::types::{Binder, Type, TypeId};

/// How many aliases `normalize` expands along one path.
const MAX_ALIASES: u32 = 64;

impl Universe {
    /// `t` with references resolved to their symbols, prefixes of classes and type parameters
    /// dropped, aliases (an abstract type's in its prefix) and applied type lambdas expanded and
    /// reduced, annotations removed and `ImpureX` read as `X`.
    pub fn normalize(&mut self, t: TypeId) -> TypeId {
        self.normalize_within(t, MAX_ALIASES)
    }

    fn normalize_within(&mut self, t: TypeId, depth: u32) -> TypeId {
        let Some(depth) = depth.checked_sub(1) else { return t };
        self.map_type(t, &mut |u, x| match u.types().get(x).clone() {
            Type::Annotated(inner, _) => Some(u.normalize_within(inner, depth)),
            Type::Applied(tycon, args) => {
                let expanded = match u.types().get(tycon).clone() {
                    Type::Lambda { binder, params, result } => {
                        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
                        u.subst(result, binder, &names, &args)
                    }
                    _ => match u
                        .expand(tycon, &args)
                        .filter(|&e| !matches!(u.types().get(e), Type::Bounds(..)))
                    {
                        Some(e) => e,
                        None => {
                            let Type::Sym(Some(prefix), s) = *u.types().get(tycon) else {
                                return None;
                            };
                            let name = u.symbol(s).name.clone();
                            let member = u.resolve(prefix, &name, true).filter(|&m| m != s)?;
                            let tycon = u.intern_type(Type::Sym(Some(prefix), member));
                            u.expand(tycon, &args)?
                        }
                    },
                };
                let expanded = u.normalized(expanded);
                Some(u.normalize_within(expanded, depth))
            }
            Type::Sym(_, _) | Type::TypeRef(_, _) => {
                let s = match u.types().get(x).clone() {
                    Type::Sym(_, s) => s,
                    Type::TypeRef(p, n) => match u.resolve(p, &n, true) {
                        Some(s) => s,
                        None => u.resolve(p, n.strip_prefix("Impure")?, true)?,
                    },
                    _ => return None,
                };
                match u.symbol(s).kind {
                    Kind::Class | Kind::TypeParam => Some(u.intern_type(Type::Sym(None, s))),
                    Kind::Type => {
                        let a = match u.alias(s) {
                            Some(a) => a,
                            None => {
                                let Type::Sym(Some(prefix), _) = *u.types().get(x) else {
                                    return None;
                                };
                                let name = u.symbol(s).name.clone();
                                let member = u.resolve(prefix, &name, true).filter(|&m| m != s)?;
                                if u.symbol(member).kind == Kind::Class {
                                    return Some(u.intern_type(Type::Sym(None, member)));
                                }
                                u.alias(member)?
                            }
                        };
                        Some(u.normalize_within(a, depth))
                    }
                    _ => None,
                }
            }
            _ => None,
        })
    }

    /// The type type alias `s` stands for, unless a type lambda.
    fn alias(&mut self, s: SymbolId) -> Option<TypeId> {
        let a = self.alias_of(s)?;
        (!matches!(self.types().get(a), Type::Lambda { .. })).then_some(a)
    }

    /// Whether methods `a` and `b`, as members of `site`, take the same numbers of type
    /// parameters and the same parameter types, `b`'s type parameters, symbols or those of its
    /// polymorphic type, renamed to `a`'s.
    pub fn matches(&mut self, site: TypeId, a: SymbolId, b: SymbolId) -> bool {
        let (ta, tb) = (self.type_params(a), self.type_params(b));
        if ta.len() != tb.len() {
            return false;
        }
        let renamed: Vec<TypeId> = ta.iter().map(|&p| self.intern_type(Type::Sym(None, p))).collect();
        let (ia, ib) = (self.member_info(site, a), self.member_info(site, b));
        let binders = match (self.types().get(ia), self.types().get(ib)) {
            (Type::Poly { binder: ba, params: pa, .. }, Type::Poly { binder: bb, params: pb, .. }) => {
                if pa.len() != pb.len() {
                    return false;
                }
                Some((*ba, *bb))
            }
            (Type::Poly { .. }, _) | (_, Type::Poly { .. }) => return false,
            _ => None,
        };
        let (ca, cb) = (self.param_clauses(ia), self.param_clauses(ib));
        let shape = |c: &[(bool, Vec<TypeId>)]| c.iter().map(|(i, ps)| (*i, ps.len())).collect::<Vec<_>>();
        if shape(&ca) != shape(&cb) {
            return false;
        }
        let pairs = ca.into_iter().zip(cb).flat_map(|((_, x), (_, y))| x.into_iter().zip(y));
        pairs.collect::<Vec<_>>().into_iter().all(|(x, y)| {
            let ((rx, x), (ry, y)) = (self.repeated_elem(x), self.repeated_elem(y));
            if rx != ry {
                return false;
            }
            let (x, y) = (self.normalize(x), self.normalize(y));
            let rename =
                |u: &mut Universe, t: TypeId, own: &[SymbolId], binders: Option<(Binder, Binder)>| {
                    u.map_type(t, &mut |u, t| match *u.types().get(t) {
                        Type::Sym(_, s) => own.iter().position(|&p| p == s).map(|i| renamed[i]),
                        Type::Param(b, i) if binders.is_some_and(|(_, own)| own == b) => {
                            let to = binders.map(|(to, _)| u.intern_type(Type::Param(to, i)));
                            renamed.get(i as usize).copied().or(to)
                        }
                        _ => None,
                    })
                };
            let x = rename(self, x, &ta, binders.map(|(ba, _)| (ba, ba)));
            let y = rename(self, y, &tb, binders);
            x == y || self.is_same_type_in(x, y, &mut crate::subtype::NoVars)
        })
    }

    /// Whether parameter type `t` is repeated, `<repeated>[T]` or `Seq[T] @Repeated` as TASTy
    /// gives it, and then its element type `T`, else `t`.
    fn repeated_elem(&mut self, t: TypeId) -> (bool, TypeId) {
        let is_class =
            |u: &mut Self, c: TypeId, name: &str| u.class_symbol(c).is_some_and(|c| u.full_name(c) == name);
        let (applied, repeated) = match self.types().get(t).clone() {
            Type::Annotated(seq, annot) => (seq, is_class(self, annot, "scala.annotation.internal.Repeated")),
            Type::Applied(c, _) => (t, is_class(self, c, "scala.<repeated>")),
            _ => return (false, t),
        };
        match self.types().get(applied) {
            Type::Applied(_, args) if repeated && args.len() == 1 => (true, args[0]),
            _ => (false, t),
        }
    }

    /// The result type of method type `t` after all its clauses.
    pub fn result_type(&self, mut t: TypeId) -> TypeId {
        while let Type::Poly { result, .. } | Type::Method { result, .. } = self.types().get(t) {
            t = *result;
        }
        t
    }

    /// The value parameter clauses of method type `t`: whether each is implicit, and its
    /// parameters' types.
    pub fn param_clauses(&self, mut t: TypeId) -> Vec<(bool, Vec<TypeId>)> {
        let mut out = Vec::new();
        loop {
            match self.types().get(t) {
                Type::Poly { result, .. } => t = *result,
                Type::Method { params, result, implicit, .. } => {
                    out.push((*implicit, params.iter().map(|(_, p)| *p).collect()));
                    t = *result;
                }
                _ => return out,
            }
        }
    }
}
