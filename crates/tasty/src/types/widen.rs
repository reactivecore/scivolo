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
            let next = match self.types().get(t).clone() {
                Type::TermRef(p, n) => match self.resolve(p, &n, false) {
                    Some(s) => self.term_type(Some(p), s),
                    None => return t,
                },
                Type::Sym(p, s) if matches!(self.symbol(s).kind, Kind::Val | Kind::Param | Kind::Def) => {
                    self.term_type(p, s)
                }
                Type::Constant(tag, _) => self.constant_type(tag),
                Type::Str(_) => self.constant_type(STRINGconst),
                Type::ByName(r) => r,
                Type::This(c) => match self.class_symbol(c) {
                    Some(c) => self.this_type(c),
                    None => return t,
                },
                _ => return t,
            };
            t = next;
        }
        t
    }

    /// `t` with aliases followed and applied type lambdas reduced, repeatedly; an alias's
    /// right-hand side is seen from the prefix of the reference.
    pub fn dealias(&mut self, mut t: TypeId) -> TypeId {
        for _ in 0..MAX_STEPS {
            let next = match self.types().get(t).clone() {
                Type::Annotated(u) => u,
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
    fn referenced_alias(&mut self, t: TypeId) -> Option<TypeId> {
        let (prefix, s) = match self.types().get(t).clone() {
            Type::Sym(p, s) => (p, s),
            Type::TypeRef(p, n) => (Some(p), self.resolve(p, &n, true)?),
            _ => return None,
        };
        let alias = self.alias_of(s)?;
        let owner = self.symbol(s).owner?;
        Some(match prefix {
            Some(p) if self.symbol(owner).kind == Kind::Class => self.as_seen_from(alias, p, owner),
            _ => alias,
        })
    }

    /// The right-hand side of type alias `s`; `None` for classes, abstract and opaque types.
    pub fn alias_of(&mut self, s: SymbolId) -> Option<TypeId> {
        if self.symbol(s).kind != Kind::Type || self.symbol(s).flags.has(OPAQUE) {
            return None;
        }
        let info = self.info(s);
        match self.types().get(info).clone() {
            Type::Alias(a) => Some(a),
            Type::Bounds(lo, hi) if lo == hi => Some(lo),
            Type::Bounds(..) | Type::Other(_) => None,
            _ => Some(info),
        }
    }

    /// The type of term `s` referred to through `prefix`.
    fn term_type(&mut self, prefix: Option<TypeId>, s: SymbolId) -> TypeId {
        let info = self.info(s);
        match (prefix, self.symbol(s).owner) {
            (Some(p), Some(owner)) if self.symbol(owner).kind == Kind::Class => {
                self.as_seen_from(info, p, owner)
            }
            _ => info,
        }
    }

    /// The type of `this` in class `c`: `c` applied to its own type parameters.
    pub fn this_type(&mut self, c: SymbolId) -> TypeId {
        let class = self.intern_type(Type::Sym(None, c));
        let params: Vec<TypeId> =
            self.type_params(c).into_iter().map(|p| self.intern_type(Type::Sym(None, p))).collect();
        match params.is_empty() {
            true => class,
            false => self.intern_type(Type::Applied(class, params)),
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
