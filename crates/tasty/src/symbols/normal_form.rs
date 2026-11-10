// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The normal form in which member types are matched.

use super::{Kind, SymbolId, Universe};
use crate::types::{Type, TypeId};

/// How many aliases `normalize` expands along one path.
const MAX_ALIASES: u32 = 64;

impl Universe {
    /// `t` with references resolved to their symbols, prefixes of classes and type parameters
    /// dropped, aliases (an abstract type's in its prefix, but `<FromJavaObject>`) and applied type
    /// lambdas expanded and reduced, annotations removed and `ImpureX` read as `X`.
    pub fn normalize(&mut self, t: TypeId) -> TypeId {
        self.normalize_within(t, MAX_ALIASES)
    }

    fn normalize_within(&mut self, t: TypeId, depth: u32) -> TypeId {
        let Some(depth) = depth.checked_sub(1) else { return t };
        self.map_type(t, &mut |u, x| match u.ty(x).clone() {
            Type::Annotated(inner, _) => Some(u.normalize_within(inner, depth)),
            Type::Applied(tycon, args) => {
                let expanded = match u.ty(tycon).clone() {
                    Type::Lambda { binder, params, result } => {
                        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
                        u.subst(result, binder, &names, &args)
                    }
                    _ => match u.expand(tycon, &args).filter(|&e| u.bounds(e).is_none()) {
                        Some(e) => e,
                        None => {
                            let Type::Sym(Some(prefix), s) = *u.ty(tycon) else {
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
                let s = match u.ty(x).clone() {
                    Type::Sym(_, s) => s,
                    Type::TypeRef(p, n) => match u.resolve(p, &n, true) {
                        Some(s) => s,
                        None => u.resolve(p, n.strip_prefix("Impure")?, true)?,
                    },
                    _ => return None,
                };
                match u.symbol(s).kind {
                    Kind::Class | Kind::TypeParam => Some(u.intern_type(Type::Sym(None, s))),
                    Kind::Type if s == u.defn().from_java_object => Some(u.intern_type(Type::Sym(None, s))),
                    Kind::Type => {
                        let a = match u.alias(s) {
                            Some(a) => a,
                            None => {
                                let Type::Sym(Some(prefix), _) = *u.ty(x) else {
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
        self.lambda(a).is_none().then_some(a)
    }
}
