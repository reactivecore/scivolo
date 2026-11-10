// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
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
            let Some(next) = self.by_name(t).or_else(|| self.widen_once(t)) else { return t };
            t = next;
        }
        t
    }

    /// The underlying type of singleton type, constant or `this` `t`; `None` for other types
    /// and unresolved paths (dotty's `underlying`, `ThisType.underlying` for `this`).
    pub fn widen_once(&mut self, t: TypeId) -> Option<TypeId> {
        Some(match *self.ty(t) {
            Type::TermRef(p, name) => match self.resolve_ref(t) {
                Some(s) => self.term_type(Some(p), s),
                None => self.term_refinement_named(p, name)?,
            },
            Type::Sym(p, s) if matches!(self.symbol(s).kind, Kind::Val | Kind::Param | Kind::Def) => {
                self.term_type(p, s)
            }
            Type::Constant(CLASSconst, bits) => {
                let class = self.class_ref("java.lang.Class");
                self.intern_applied(class, &[TypeId(bits as u32)])
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
            match self.dealias_step(t) {
                next if next == t => return t,
                next => t = next,
            }
        }
        t
    }

    /// `t` with its alias followed or its applied type lambda reduced once; `t` if neither.
    pub fn dealias_step(&mut self, t: TypeId) -> TypeId {
        match *self.ty(t) {
            Type::Annotated(u, _) => u,
            Type::Applied(tycon, args) => match *self.ty(tycon) {
                Type::Lambda { binder, params, result } => {
                    let names: Vec<_> = self.params_of(params).iter().map(|&(n, _)| n).collect();
                    let args = self.types_of(args).to_vec();
                    self.subst(result, binder, &names, &args)
                }
                _ => Some(self.dealias_step(tycon))
                    .filter(|&d| d != tycon)
                    .map_or(t, |d| self.intern_type(Type::Applied(d, args))),
            },
            Type::Sym(..) | Type::TypeRef(..) => self.referenced_alias(t).unwrap_or(t),
            _ => t,
        }
    }

    /// Determines the right-hand side of the type alias that reference `t` names, seen from its
    /// prefix: an alias in a refinement of the prefix (`C { type T = Int }`), an opaque type seen
    /// from inside its scope, a plain alias, or the alias by which the prefix's class overrides an
    /// abstract type member. `None` for classes and abstract types.
    pub fn referenced_alias(&mut self, t: TypeId) -> Option<TypeId> {
        let (prefix, s) = match *self.ty(t) {
            Type::Sym(p, s) => (p, s),
            Type::TypeRef(p, n) => match self.resolve_ref(t) {
                Some(s) => (Some(p), s),
                None => return self.refinement_named(p, n).and_then(|i| self.type_alias(i)),
            },
            _ => return None,
        };
        if let Some(info) = prefix.and_then(|p| self.refinement_info(p, s))
            && let Some(a) = self.type_alias(info)
        {
            return Some(a);
        }
        let alias = match self.visible_opaque_alias(s, prefix) {
            Some(a) => a,
            None => match self.alias_of(s) {
                Some(a) => a,
                None => return self.overriding_alias(prefix?, s),
            },
        };
        let owner = self.symbol(s).owner?;
        Some(match prefix {
            Some(p) if self.symbol(owner).kind == Kind::Class => self.as_seen_from(alias, p, owner),
            _ => alias,
        })
    }

    /// The right-hand side of type alias `s`; `None` for classes, abstract types (higher-kinded
    /// ones included) and opaque types.
    pub fn alias_of(&mut self, s: SymbolId) -> Option<TypeId> {
        if self.symbol(s).kind != Kind::Type || self.symbol(s).flags.has(OPAQUE) {
            return None;
        }
        let info = self.info(s);
        match *self.ty(info) {
            Type::Alias(a) => Some(a),
            Type::Bounds(..) | Type::Other(_) => None,
            Type::Lambda { result, .. } if self.bounds(result).is_some() => None,
            _ => Some(info),
        }
    }

    /// The type of term `s` referred to through `prefix`.
    pub fn term_type(&mut self, prefix: Option<TypeId>, s: SymbolId) -> TypeId {
        let info = self.info(s);
        match (prefix, self.symbol(s).owner) {
            (Some(p), Some(owner)) if self.symbol(owner).kind == Kind::Class => self.member_info(p, s),
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
            false => self.intern_applied(class, &params),
        }
    }

    /// The type of `this` in class `c` with its given self type (dotty's `ClassInfo.selfType`).
    pub fn self_type(&mut self, c: SymbolId) -> TypeId {
        let applied = self.this_type(c);
        self.given_self_type(c).map_or(applied, |s| self.intern_type(Type::And(s, applied)))
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
                    None => {
                        let pkg = self.intern_name(pkg);
                        self.intern_type(Type::PackageName(pkg))
                    }
                };
                let name = self.intern_name(name);
                self.intern_type(Type::TypeRef(prefix, name))
            }
        }
    }
}
