//! Widening singleton types and following aliases with their prefixes kept (dotty's `widen` and
//! `dealias`).

use super::{Type, TypeId};
use crate::tags::{
    BYTEconst, CHARconst, CLASSconst, DOUBLEconst, FALSEconst, FLOATconst, INTconst, LONGconst, NULLconst,
    OPAQUE, SHORTconst, STRINGconst, TRUEconst, UNITconst,
};
use crate::{Kind, SymbolId, Universe};

/// How many steps `widen` and `dealias` take at most.
const MAX_STEPS: u32 = 64;

impl Universe {
    /// `t` with singleton types, constants, `this` and by-name types replaced by their
    /// underlying types, repeatedly.
    pub fn widen(&mut self, mut t: TypeId) -> TypeId {
        for _ in 0..MAX_STEPS {
            let next = match *self.types().get(t) {
                Type::ByName(r) => Some(r),
                _ => self.widen_once(t),
            };
            let Some(next) = next else { return t };
            t = next;
        }
        t
    }

    /// The underlying type of singleton type, constant or `this` `t`; `None` for other types
    /// and unresolved paths.
    pub fn widen_once(&mut self, t: TypeId) -> Option<TypeId> {
        Some(match self.types().get(t).clone() {
            Type::TermRef(p, _) => {
                let s = self.resolve_ref(t)?;
                self.term_type(Some(p), s)
            }
            Type::Sym(p, s) if matches!(self.symbol(s).kind, Kind::Val | Kind::Param | Kind::Def) => {
                self.term_type(p, s)
            }
            Type::Constant(CLASSconst, bits) => {
                let class = self.class_ref("java.lang.Class");
                self.intern_type(Type::Applied(class, vec![TypeId(bits as u32)]))
            }
            Type::Constant(tag, _) => self.constant_type(tag),
            Type::Str(_) => self.constant_type(STRINGconst),
            Type::This(c) => {
                let c = self.class_symbol(c)?;
                self.self_type(c)
            }
            Type::Super(_, p) | Type::Skolem(_, p) => p,
            _ => return None,
        })
    }

    /// `t` with aliases followed and applied type lambdas reduced, repeatedly; an alias's
    /// right-hand side is seen from the prefix of the reference.
    pub fn dealias(&mut self, mut t: TypeId) -> TypeId {
        for _ in 0..MAX_STEPS {
            let next = match self.types().get(t).clone() {
                Type::Annotated(u, _) => u,
                Type::Applied(tycon, args) => match self.types().get(tycon).clone() {
                    Type::Lambda { binder, params, result } => {
                        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
                        self.subst(result, binder, &names, &args)
                    }
                    _ => match self.referenced_alias(tycon) {
                        Some(_) => match self.expand(tycon, &args) {
                            Some(e) => e,
                            None => return t,
                        },
                        None => return t,
                    },
                },
                Type::Sym(..) | Type::TypeRef(..) => match self.referenced_alias(t) {
                    Some(a) => a,
                    None => return t,
                },
                _ => return t,
            };
            t = next;
        }
        t
    }

    /// The right-hand side of the alias `t` refers to, seen from the reference's prefix.
    pub fn referenced_alias(&mut self, t: TypeId) -> Option<TypeId> {
        let (prefix, s) = match self.types().get(t).clone() {
            Type::Sym(p, s) => (p, s),
            Type::TypeRef(p, _) => (Some(p), self.resolve_ref(t)?),
            _ => return None,
        };
        if let Some(info) = prefix.and_then(|p| self.refinement_info(p, s))
            && let Type::Alias(a) = *self.types().get(info)
        {
            return Some(a);
        }
        let alias = match self.visible_opaque_alias(s, prefix) {
            Some(a) => a,
            None => self.alias_of(s)?,
        };
        let owner = self.symbol(s).owner?;
        Some(match prefix {
            Some(p) if self.symbol(owner).kind == Kind::Class => self.as_seen_from(alias, p, owner),
            _ => alias,
        })
    }

    /// The info a refinement of `prefix`'s widened type gives type member `s` (dotty's
    /// `goRefined`).
    pub fn refinement_info(&mut self, prefix: TypeId, s: SymbolId) -> Option<TypeId> {
        if self.symbol(s).kind != Kind::Type {
            return None;
        }
        let name = self.symbol(s).name.clone();
        let mut t = self.widen(prefix);
        for _ in 0..MAX_STEPS {
            match self.types().get(t).clone() {
                Type::Refined(_, n, info)
                    if n == name && matches!(self.types().get(info), Type::Alias(_) | Type::Bounds(..)) =>
                {
                    return Some(info);
                }
                Type::Refined(p, ..) | Type::Annotated(p, _) => t = p,
                _ => match self.dealias(t) {
                    d if d != t => t = d,
                    _ => return None,
                },
            }
        }
        None
    }

    /// The right-hand side of type alias `s`; `None` for classes, abstract types (higher-kinded
    /// ones included) and opaque types.
    pub fn alias_of(&mut self, s: SymbolId) -> Option<TypeId> {
        if self.symbol(s).kind != Kind::Type || self.symbol(s).flags.has(OPAQUE) {
            return None;
        }
        let info = self.info(s);
        match self.types().get(info).clone() {
            Type::Alias(a) => Some(a),
            Type::Bounds(..) | Type::Other(_) => None,
            Type::Lambda { result, .. } if matches!(self.types().get(result), Type::Bounds(..)) => None,
            _ => Some(info),
        }
    }

    /// The type of term `s` referred to through `prefix`.
    pub fn term_type(&mut self, prefix: Option<TypeId>, s: SymbolId) -> TypeId {
        let info = self.info(s);
        match (prefix, self.symbol(s).owner) {
            (Some(p), Some(owner)) if self.symbol(owner).kind == Kind::Class => {
                self.as_seen_from(info, p, owner)
            }
            _ => info,
        }
    }

    /// The type of `this` in class `c`: `c` applied to its own type parameters, seen from `this`
    /// (dotty's `appliedRef`).
    pub fn this_type(&mut self, c: SymbolId) -> TypeId {
        let class = self.intern_type(Type::Sym(None, c));
        let this = self.intern_type(Type::This(class));
        let params: Vec<TypeId> =
            self.type_params(c).into_iter().map(|p| self.intern_type(Type::Sym(Some(this), p))).collect();
        match params.is_empty() {
            true => class,
            false => self.intern_type(Type::Applied(class, params)),
        }
    }

    /// The type of `this` in class `c` with its given self type (dotty's `ClassInfo.selfType`).
    pub fn self_type(&mut self, c: SymbolId) -> TypeId {
        let applied = self.this_type(c);
        match self.given_self_type(c) {
            Some(s) => self.intern_type(Type::And(s, applied)),
            None => applied,
        }
    }

    /// The class type of a constant with tag `tag`.
    fn constant_type(&mut self, tag: u8) -> TypeId {
        let name = match tag {
            UNITconst => "scala.Unit",
            TRUEconst | FALSEconst => "scala.Boolean",
            BYTEconst => "scala.Byte",
            SHORTconst => "scala.Short",
            CHARconst => "scala.Char",
            INTconst => "scala.Int",
            LONGconst => "scala.Long",
            FLOATconst => "scala.Float",
            DOUBLEconst => "scala.Double",
            STRINGconst => "java.lang.String",
            NULLconst => "scala.Null",
            CLASSconst => "java.lang.Class",
            _ => "scala.Any",
        };
        self.class_ref(name)
    }

    /// A reference to the class of qualified name `full`, resolved if the universe has it.
    pub fn class_ref(&mut self, full: &str) -> TypeId {
        let (pkg, name) = full.rsplit_once('.').unwrap_or(("", full));
        let class = self
            .package(pkg)
            .and_then(|p| self.decl(p, name).ok()?.into_iter().find(|&s| self.symbol(s).kind == Kind::Class));
        match class {
            Some(c) => self.intern_type(Type::Sym(None, c)),
            None => {
                let prefix = match self.package(pkg) {
                    Some(p) => self.intern_type(Type::Package(p)),
                    None => self.intern_type(Type::PackageName(pkg.to_owned())),
                };
                self.intern_type(Type::TypeRef(prefix, name.to_owned()))
            }
        }
    }
}
