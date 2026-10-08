//! Whether one method's signature matches another's as members of the same class (dotty's
//! `matches`), on types in a normal form.

use super::{Kind, SymbolId, Universe};
use crate::types::{Type, TypeId};

/// How many aliases `normalize` expands along one path.
const MAX_ALIASES: u32 = 64;

impl Universe {
    /// `t` with references resolved to their symbols, prefixes of classes and type parameters
    /// dropped, aliases (an abstract type's in its prefix) and applied type lambdas expanded,
    /// annotations removed and `ImpureX` read as `X`.
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
                    _ => u.expand(tycon, &args)?,
                };
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
                                let Type::Sym(Some(prefix), _) = u.types().get(x).clone() else {
                                    return None;
                                };
                                let name = u.symbol(s).name.clone();
                                let member = u.resolve(prefix, &name, true).filter(|&m| m != s)?;
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
        let info = self.info(s);
        match self.types().get(info) {
            &Type::Alias(a) if !matches!(self.types().get(a), Type::Lambda { .. }) => Some(a),
            _ => None,
        }
    }

    /// Whether methods `a` and `b`, as members of `site`, take the same numbers of type
    /// parameters and the same parameter types, `b`'s type parameters renamed to `a`'s.
    pub fn matches(&mut self, site: TypeId, a: SymbolId, b: SymbolId) -> bool {
        let (ta, tb) = (self.type_params(a), self.type_params(b));
        if ta.len() != tb.len() {
            return false;
        }
        let renamed: Vec<TypeId> = ta.iter().map(|&p| self.intern_type(Type::Sym(None, p))).collect();
        let (ia, ib) = (self.member_info(site, a), self.member_info(site, b));
        let (ca, cb) = (self.param_clauses(ia), self.param_clauses(ib));
        let shape = |c: &[(bool, Vec<TypeId>)]| c.iter().map(|(i, ps)| (*i, ps.len())).collect::<Vec<_>>();
        if shape(&ca) != shape(&cb) {
            return false;
        }
        let pairs = ca.into_iter().zip(cb).flat_map(|((_, x), (_, y))| x.into_iter().zip(y));
        pairs.collect::<Vec<_>>().into_iter().all(|(x, y)| {
            let (x, y) = (self.normalize(x), self.normalize(y));
            let y = self.map_type(y, &mut |u, t| match u.types().get(t) {
                Type::Sym(_, s) => tb.iter().position(|p| p == s).map(|i| renamed[i]),
                _ => None,
            });
            x == y
        })
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
