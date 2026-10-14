//! The members dotty's `Definitions` gives `Any`, `java.lang.Object`, whose members repeating
//! `Any`'s are hidden, and `java.lang.String`.

use super::builtin::synthetic;
use super::{Kind, SymbolId, Universe};
use crate::tags::{ARTIFACT, FINAL, PROTECTED};
use crate::types::{Type, TypeId};
use crate::{ANY, NOTHING};

/// The members of `Object`'s TASTy that `Any` declares.
const HIDDEN: [&str; 4] = ["equals", "hashCode", "toString", "getClass"];

/// The members of `Any` that do not observe their arguments (dotty's `pureMethods`).
const PURE_ANY_METHODS: [&str; 9] =
    ["==", "!=", "equals", "hashCode", "toString", "##", "getClass", "isInstanceOf", "$isInstanceOf"];

impl Universe {
    /// Whether method `m` is known not to observe its arguments (dotty's `isKnownPureOp`): a
    /// method of a primitive class or `String`, or one of `pureMethods`.
    pub fn is_known_pure_op(&self, m: SymbolId) -> bool {
        let Some(owner) = self.symbol(m).owner else { return false };
        let name = self.symbol(m).name.as_str();
        let full = self.full_name(owner);
        let builtins = self.builtins();
        crate::PRIMITIVE_VALUE_CLASSES.contains(&full.as_str())
            || full == "java.lang.String"
            || owner == builtins.any && PURE_ANY_METHODS.contains(&name)
            || owner == builtins.object && matches!(name, "eq" | "ne")
    }
}

/// Enters the members of `any` and `object`, and `String`'s `+`; `object`'s TASTy, if loaded,
/// keeps its members but those `Any` declares.
pub fn enter(u: &mut Universe, any: SymbolId, object: SymbolId) {
    let boolean = class_type(u, "scala", "Boolean");
    let int = class_type(u, "scala", "Int");
    let string = class_type(u, "java.lang", "String");
    let object_type = u.types.intern(Type::Sym(None, object));
    method(u, any, "==", &[("x$0", ANY)], boolean, &[FINAL]);
    method(u, any, "!=", &[("x$0", ANY)], boolean, &[FINAL]);
    method(u, any, "equals", &[("x$0", ANY)], boolean, &[]);
    method(u, any, "hashCode", &[], int, &[]);
    method(u, any, "toString", &[], string, &[]);
    value_method(u, any, "##", int, &[FINAL]);
    let any_bounds = u.types.intern(Type::Bounds(NOTHING, ANY));
    let class = class_type(u, "java.lang", "Class");
    let any_ref = u.types.intern(Type::Sym(None, any));
    let this_any = u.types.intern(Type::This(any_ref));
    let get_class_bounds = u.types.intern(Type::Bounds(this_any, ANY));
    poly(u, any, "getClass", get_class_bounds, |u, x0| {
        let x0 = u.types.intern(x0.clone());
        let bounds = u.types.intern(Type::Bounds(NOTHING, x0));
        let result = u.types.intern(Type::Applied(class, vec![bounds]));
        let binder = u.fresh_binder();
        u.types.intern(Type::Method { binder, params: vec![], result, implicit: false, contextual: false })
    });
    poly(u, any, "isInstanceOf", any_bounds, |_, _| boolean);
    let type_test = poly(u, any, "$isInstanceOf", any_bounds, |_, _| boolean);
    u.set_flags(type_test, u.symbol(type_test).flags.with(ARTIFACT));
    poly(u, any, "asInstanceOf", any_bounds, |u, x0| u.types.intern(x0.clone()));
    let type_cast = poly(u, any, "$asInstanceOf$", any_bounds, |u, x0| u.types.intern(x0.clone()));
    u.set_flags(type_cast, u.symbol(type_cast).flags.with(ARTIFACT));
    let hidden: Vec<SymbolId> = (u.symbol(object).decls.iter().copied())
        .filter(|&d| HIDDEN.contains(&u.symbol(d).name.as_str()))
        .collect();
    hidden.into_iter().for_each(|d| u.unenter(d));
    method(u, object, "eq", &[("x$0", object_type)], boolean, &[FINAL]);
    method(u, object, "ne", &[("x$0", object_type)], boolean, &[FINAL]);
    poly(u, object, "synchronized", any_bounds, |u, x0| {
        let t = u.types.intern(x0.clone());
        let binder = u.fresh_binder();
        u.types.intern(Type::Method {
            binder,
            params: vec![("x$0".into(), t)],
            result: t,
            implicit: false,
            contextual: false,
        })
    });
    if u.symbol(object).decls.iter().all(|&d| u.symbol(d).name != "clone") {
        method(u, object, "clone", &[], object_type, &[PROTECTED]);
    }
    let string_class = u
        .package("java.lang")
        .and_then(|p| u.decl(p, "String").ok()?.into_iter().find(|&s| u.symbol(s).kind == Kind::Class));
    if let Some(s) = string_class {
        method(u, s, "+", &[("x$0", ANY)], string, &[FINAL]);
    }
}

/// A reference to class `name` of package `pkg`, resolved when used.
pub fn class_type(u: &mut Universe, pkg: &str, name: &str) -> TypeId {
    let prefix = match u.package(pkg) {
        Some(p) => u.types.intern(Type::Package(p)),
        None => u.types.intern(Type::PackageName(pkg.into())),
    };
    u.types.intern(Type::TypeRef(prefix, name.into()))
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
    let params = params.iter().map(|&(n, t)| (n.to_owned(), t)).collect();
    let info = u.types.intern(Type::Method { binder, params, result, implicit: false, contextual: false });
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
    let info = u.types.intern(Type::Poly { binder, params: vec![("X0".into(), bounds)], result });
    let s = synthetic(u, owner, name, Kind::Def, &[FINAL]);
    u.infos.insert(s, info);
    s
}
