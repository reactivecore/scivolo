//! Whether one method's signature matches another's as members of the same class (dotty's
//! `matches`), on types in a normal form.

use super::{Kind, SymbolId, Universe};
use crate::types::{Type, TypeId};

/// How many aliases `normalize` expands along one path.
const MAX_ALIASES: u32 = 64;

impl Universe {
    /// `t` with references resolved to their symbols, prefixes of classes and type parameters
    /// dropped, aliases and applied type lambdas expanded, annotations removed and `ImpureX`
    /// read as `X`.
    pub fn normalize(&mut self, t: TypeId) -> TypeId {
        self.normalize_within(t, MAX_ALIASES)
    }

    fn normalize_within(&mut self, t: TypeId, depth: u32) -> TypeId {
        let Some(depth) = depth.checked_sub(1) else { return t };
        self.map_type(t, &mut |u, x| match u.types().get(x).clone() {
            Type::Annotated(inner) => Some(u.normalize_within(inner, depth)),
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
                        let info = u.info(s);
                        match u.types().get(info).clone() {
                            Type::Alias(a) if !matches!(u.types().get(a), Type::Lambda { .. }) => {
                                Some(u.normalize_within(a, depth))
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        })
    }

    /// Whether methods `a` and `b`, as members of `site`, take the same numbers of type
    /// parameters and the same parameter types, `b`'s type parameters renamed to `a`'s.
    pub fn matches(&mut self, site: TypeId, a: SymbolId, b: SymbolId) -> bool {
        let (ta, tb) = (self.type_params_of(a), self.type_params_of(b));
        if ta.len() != tb.len() {
            return false;
        }
        let renamed: Vec<TypeId> = ta.iter().map(|&p| self.intern_type(Type::Sym(None, p))).collect();
        let (ia, ib) = (self.member_info(site, a), self.member_info(site, b));
        let (pa, pb) = (self.param_types(ia), self.param_types(ib));
        pa.len() == pb.len()
            && pa.into_iter().zip(pb).all(|(x, y)| {
                let (x, y) = (self.normalize(x), self.normalize(y));
                let y = self.map_type(y, &mut |u, t| match u.types().get(t) {
                    Type::Sym(_, s) => tb.iter().position(|p| p == s).map(|i| renamed[i]),
                    _ => None,
                });
                x == y
            })
    }

    /// The type parameters of method `m`.
    pub fn type_params_of(&self, m: SymbolId) -> Vec<SymbolId> {
        let decls = &self.symbol(m).decls;
        decls.iter().copied().filter(|&d| self.symbol(d).kind == Kind::TypeParam).collect()
    }

    /// The result type of method type `t` after all its clauses.
    pub fn result_type(&self, mut t: TypeId) -> TypeId {
        while let Type::Poly { result, .. } | Type::Method { result, .. } = self.types().get(t) {
            t = *result;
        }
        t
    }

    /// The types of the value parameters of method type `t`, all clauses.
    pub fn param_types(&self, mut t: TypeId) -> Vec<TypeId> {
        let mut out = Vec::new();
        loop {
            match self.types().get(t) {
                Type::Poly { result, .. } => t = *result,
                Type::Method { params, result, .. } => {
                    out.extend(params.iter().map(|(_, p)| *p));
                    t = *result;
                }
                _ => return out,
            }
        }
    }
}
