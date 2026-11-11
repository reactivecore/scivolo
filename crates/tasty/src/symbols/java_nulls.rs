// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/ImplicitNullInterop.scala
//! The types of JDK members under explicit nulls: their references flexible (dotty's
//! `ImplicitNullInterop.nullifyMember`).

use super::{Kind, SymbolId, Universe};
use crate::tags::{ENUM, GIVEN, IMPLICIT};
use crate::types::{Type, TypeId};

/// Whether the type being mapped and the result of a method type in it are made flexible.
#[derive(Clone, Copy)]
struct Modes {
    current: bool,
    result: bool,
}

impl Universe {
    /// Type `t` of member `def` with its references made flexible if `def` is a JDK member.
    pub fn nullified_member(&mut self, def: SymbolId, t: TypeId) -> TypeId {
        let s = self.symbol(def);
        let Some(owner) = s.owner else { return t };
        let skipped = matches!(s.name.as_str(), "TYPE" | "getClass" | "toString")
            || !matches!(s.kind, Kind::Def | Kind::Val)
            || s.flags.has(ENUM)
            || self.is_module(def)
            || self.is_entered_object_member(def);
        if skipped || !self.is_jdk_class(owner) || !self.may_be_java(owner) {
            return t;
        }
        let current = !self.symbol(def).flags.has(GIVEN) && !self.symbol(def).flags.has(IMPLICIT);
        let modes = Modes { current, result: current && !self.is_constructor(def) };
        self.nullify(t, modes)
    }

    /// Whether class `c` is read from the JDK's class files in Scala Native's build: one javalib
    /// defines under another name, one of the JDK jars', or, in the build of a library other
    /// than javalib that JDK jars stand for, any of a JDK package; or one entered from a JDK
    /// class file.
    fn is_jdk_class(&self, c: SymbolId) -> bool {
        if self.is_from_classfile(c) {
            return true;
        }
        let name = self.full_name(c);
        let name = name.strip_suffix('$').unwrap_or(&name);
        let jdk_package = name.starts_with("java.") || name.starts_with("javax.");
        let library_build = !self.jdk_jars.is_empty() && !self.javalib_sources();
        super::jdk::RENAMED.iter().any(|&(_, jdk)| jdk == name)
            || self.is_from_jdk_jars(c)
            || jdk_package && library_build
    }

    fn nullify(&mut self, t: TypeId, modes: Modes) -> TypeId {
        match *self.ty(t) {
            Type::Sym(..) | Type::TypeRef(..) | Type::Param(..) => self.nullified(t, modes),
            Type::Applied(tycon, args) => {
                let java = self.class_symbol(tycon).is_some_and(|c| self.is_jdk_class(c));
                let inner = Modes { current: !java, result: true };
                let args: Vec<_> =
                    self.types_of(args).to_vec().into_iter().map(|a| self.nullify(a, inner)).collect();
                let applied = self.intern_applied(tycon, &args);
                self.nullified(applied, modes)
            }
            Type::Annotated(inner, annot) => {
                let inner = self.nullify(inner, modes);
                let repeated = self.class_symbol(annot).map(|c| self.full_name(c));
                let repeated = repeated.as_deref() == Some("scala.annotation.internal.Repeated");
                match *self.ty(inner) {
                    Type::Flexible(hi) if repeated => self.intern_type(Type::Annotated(hi, annot)),
                    Type::Flexible(hi) => {
                        let annotated = self.intern_type(Type::Annotated(hi, annot));
                        self.flexible(annotated)
                    }
                    _ => self.intern_type(Type::Annotated(inner, annot)),
                }
            }
            Type::Poly { binder, params, result } => {
                let result = self.nullify(result, modes);
                self.intern_type(Type::Poly { binder, params, result })
            }
            Type::Method { binder, params, result, implicit, contextual } => {
                let inner = Modes { current: !implicit, result: true };
                let params: Vec<_> = self
                    .params_of(params)
                    .to_vec()
                    .into_iter()
                    .map(|(n, p)| (n, self.nullify(p, inner)))
                    .collect();
                let result = self.nullify(result, Modes { current: modes.result, result: true });
                self.intern_method(binder, &params, result, implicit, contextual)
            }
            Type::ByName(r) => {
                let r = self.nullify(r, modes);
                self.intern_type(Type::ByName(r))
            }
            Type::Alias(a) => {
                let a = self.nullify(a, modes);
                self.intern_type(Type::Alias(a))
            }
            Type::Bounds(lo, hi) => {
                let (lo, hi) = (self.nullify(lo, modes), self.nullify(hi, modes));
                self.intern_type(Type::Bounds(lo, hi))
            }
            Type::And(a, b) | Type::Or(a, b, _) => {
                let (x, y) = (self.nullify(a, modes), self.nullify(b, modes));
                let join = |u: &mut Self, x, y| match *u.ty(t) {
                    Type::Or(_, _, soft) => u.intern_type(Type::Or(x, y, soft)),
                    _ => u.intern_type(Type::And(x, y)),
                };
                match (*self.ty(x), *self.ty(y)) {
                    (Type::Flexible(x), Type::Flexible(y)) => {
                        let joined = join(self, x, y);
                        self.flexible(joined)
                    }
                    _ => join(self, x, y),
                }
            }
            Type::Refined(parent, name, info) => {
                let parent = self.nullify(parent, modes);
                match *self.ty(parent) {
                    Type::Flexible(hi) => {
                        let refined = self.intern_type(Type::Refined(hi, name, info));
                        self.flexible(refined)
                    }
                    _ => self.intern_type(Type::Refined(parent, name, info)),
                }
            }
            _ => t,
        }
    }

    /// `t?` if `t` may hold `null` in Java and `modes` makes it flexible (dotty's `needsNull`).
    fn nullified(&mut self, t: TypeId, modes: Modes) -> TypeId {
        if !modes.current || self.needs_no_null(t) {
            return t;
        }
        self.flexible(t)
    }

    fn needs_no_null(&mut self, t: TypeId) -> bool {
        let t = self.dealias(t);
        match *self.ty(t) {
            Type::Sym(..) | Type::TypeRef(..) => {
                let Some(s) = self.referenced(t) else { return false };
                let b = self.defn();
                let special = [b.null, b.nothing, b.any, b.any_kind, b.singleton];
                let unit = self.full_name(s) == super::defn::UNIT;
                let value = b.any_val.is_some_and(|v| s == v || self.derives_from(s, v));
                special.contains(&s) || unit || value || self.symbol(s).kind == Kind::Class && self.is_hk(s)
            }
            Type::Param(..) => false,
            Type::Applied(tycon, _) => {
                self.class_symbol(tycon).is_some_and(|c| self.full_name(c).starts_with("scala.Tuple"))
            }
            _ => true,
        }
    }

    /// Whether class `c` takes type parameters (a reference to it is not of simple kind).
    fn is_hk(&mut self, c: SymbolId) -> bool {
        !self.type_params(c).is_empty()
    }
}
