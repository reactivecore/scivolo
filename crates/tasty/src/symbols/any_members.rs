// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//! The members dotty's `Definitions` gives `Any`, `java.lang.Object`, whose members repeating
//! `Any`'s are hidden, and `java.lang.String`.

use super::builtin::synthetic;
use super::{Kind, SymbolId, Universe};
use crate::tags::{ARTIFACT, FINAL, INFIX, PROTECTED};
use crate::types::{Type, TypeId};
use crate::{ANY, NOTHING};
use names::nme;

/// The members of `Object`'s TASTy that `Any` declares.
const HIDDEN: [&str; 4] = [nme::EQUALS, nme::HASH_CODE, nme::TO_STRING, nme::GET_CLASS];

/// The members of `Any` that do not observe their arguments (dotty's `pureMethods`).
const PURE_ANY_METHODS: [&str; 9] = [
    nme::EQ,
    nme::NE,
    nme::EQUALS,
    nme::HASH_CODE,
    nme::TO_STRING,
    nme::HASHHASH,
    nme::GET_CLASS,
    nme::IS_INSTANCE_OF,
    nme::IS_INSTANCE_OF_PM,
];

impl Universe {
    /// Whether method `m` is known not to observe its arguments (dotty's `isKnownPureOp`): a
    /// method of a primitive class or `String`, or one of `pureMethods`.
    pub fn is_known_pure_op(&self, m: SymbolId) -> bool {
        let Some(owner) = self.symbol(m).owner else { return false };
        let name = self.symbol(m).name.as_str();
        let full = self.full_name(owner);
        let builtins = self.defn();
        crate::PRIMITIVE_VALUE_CLASSES.contains(&full.as_str())
            || full == "java.lang.String"
            || owner == builtins.any && PURE_ANY_METHODS.contains(&name)
            || owner == builtins.object && matches!(name, nme::REF_EQ | nme::REF_NE)
    }

    /// Whether `m` is one of the members `enter` gives `Object`, which are Scala ones.
    pub fn is_entered_object_member(&self, m: SymbolId) -> bool {
        let s = self.symbol(m);
        s.owner == Some(self.defn().object)
            && matches!(s.name.as_str(), nme::REF_EQ | nme::REF_NE | nme::SYNCHRONIZED)
    }
}

/// Enters the members of `any` and `object`, and `String`'s `+`; `object`'s TASTy, if loaded,
/// keeps its members but those `Any` declares.
pub fn enter(u: &mut Universe, any: SymbolId, object: SymbolId) {
    let boolean = class_type(u, "scala", "Boolean");
    let int = class_type(u, "scala", "Int");
    let string = class_type(u, "java.lang", "String");
    let object_type = u.types.intern(Type::Sym(None, object));
    method(u, any, nme::EQ, &[(nme::X_0, ANY)], boolean, &[FINAL]);
    method(u, any, nme::NE, &[(nme::X_0, ANY)], boolean, &[FINAL]);
    method(u, any, nme::EQUALS, &[(nme::X_0, ANY)], boolean, &[]);
    method(u, any, nme::HASH_CODE, &[], int, &[]);
    method(u, any, nme::TO_STRING, &[], string, &[]);
    value_method(u, any, nme::HASHHASH, int, &[FINAL]);
    let any_bounds = u.types.intern(Type::Bounds(NOTHING, ANY));
    let class = class_type(u, "java.lang", "Class");
    let any_ref = u.types.intern(Type::Sym(None, any));
    let this_any = u.types.intern(Type::This(any_ref));
    let get_class_bounds = u.types.intern(Type::Bounds(this_any, ANY));
    poly(u, any, nme::GET_CLASS, get_class_bounds, |u, x0| {
        let x0 = u.types.intern(*x0);
        let bounds = u.types.intern(Type::Bounds(NOTHING, x0));
        let result = u.types.intern_applied(class, &[bounds]);
        let binder = u.fresh_binder();
        u.types.intern_method(binder, &[], result, false, false)
    });
    poly(u, any, nme::IS_INSTANCE_OF, any_bounds, |_, _| boolean);
    let type_test = poly(u, any, nme::IS_INSTANCE_OF_PM, any_bounds, |_, _| boolean);
    u.set_flags(type_test, u.symbol(type_test).flags.with(ARTIFACT));
    poly(u, any, nme::AS_INSTANCE_OF, any_bounds, |u, x0| u.types.intern(*x0));
    let type_cast = poly(u, any, nme::AS_INSTANCE_OF_PM, any_bounds, |u, x0| u.types.intern(*x0));
    u.set_flags(type_cast, u.symbol(type_cast).flags.with(ARTIFACT));
    let hidden: Vec<SymbolId> = (u.symbol(object).decls.iter().copied())
        .filter(|&d| HIDDEN.contains(&u.symbol(d).name.as_str()))
        .collect();
    hidden.into_iter().for_each(|d| u.unenter(d));
    method(u, object, nme::REF_EQ, &[(nme::X_0, object_type)], boolean, &[FINAL, INFIX]);
    method(u, object, nme::REF_NE, &[(nme::X_0, object_type)], boolean, &[FINAL, INFIX]);
    poly(u, object, nme::SYNCHRONIZED, any_bounds, |u, x0| {
        let t = u.types.intern(*x0);
        let binder = u.fresh_binder();
        let x0 = u.types.intern_name(nme::X_0);
        u.types.intern_method(binder, &[(x0, t)], t, false, false)
    });
    if u.symbol(object).decls.iter().all(|&d| u.symbol(d).name != nme::CLONE) {
        method(u, object, nme::CLONE, &[], object_type, &[PROTECTED]);
    }
    let string_class = u
        .package("java.lang")
        .and_then(|p| u.decl(p, "String").ok()?.into_iter().find(|&s| u.symbol(s).kind == Kind::Class));
    if let Some(s) = string_class {
        method(u, s, nme::ADD, &[(nme::X_0, ANY)], string, &[FINAL]);
    }
}

/// A reference to class `name` of package `pkg`, resolved when used.
pub fn class_type(u: &mut Universe, pkg: &str, name: &str) -> TypeId {
    let prefix = match u.package(pkg) {
        Some(p) => u.types.intern(Type::Package(p)),
        None => {
            let pkg = u.types.intern_name(pkg);
            u.types.intern(Type::PackageName(pkg))
        }
    };
    let name = u.types.intern_name(name);
    u.types.intern(Type::TypeRef(prefix, name))
}

pub fn method(
    u: &mut Universe,
    owner: SymbolId,
    name: &str,
    params: &[(&str, TypeId)],
    result: TypeId,
    flags: &[u8],
) {
    let binder = u.fresh_binder();
    let params: Vec<_> = params.iter().map(|&(n, t)| (u.types.intern_name(n), t)).collect();
    let info = u.types.intern_method(binder, &params, result, false, false);
    let s = synthetic(u, owner, name, Kind::Def, flags);
    u.infos.insert(s, info);
}

/// A method without parameter lists.
fn value_method(u: &mut Universe, owner: SymbolId, name: &str, result: TypeId, flags: &[u8]) {
    let s = synthetic(u, owner, name, Kind::Def, flags);
    u.infos.insert(s, result);
}

/// A final method with one type parameter `X0` of bounds `bounds`, its type made by `result`
/// from the parameter.
fn poly(
    u: &mut Universe,
    owner: SymbolId,
    name: &str,
    bounds: TypeId,
    result: impl FnOnce(&mut Universe, &Type) -> TypeId,
) -> SymbolId {
    let binder = u.fresh_binder();
    let x0 = Type::Param(binder, 0);
    let result = result(u, &x0);
    let params = [(u.types.intern_name("X0"), bounds)];
    let info = u.types.intern_poly(binder, &params, result);
    let s = synthetic(u, owner, name, Kind::Def, &[FINAL]);
    u.infos.insert(s, info);
    s
}
