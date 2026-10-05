//! The members dotty's `Definitions` gives `Any`, `java.lang.Object`, whose members repeating
//! `Any`'s are hidden, and `java.lang.String`.

use super::builtin::synthetic;
use super::{Kind, SymbolId, Universe};
use crate::tags::{FINAL, PROTECTED};
use crate::types::{Type, TypeId};
use crate::{ANY, NOTHING};

/// The members of `Object`'s TASTy that `Any` declares.
const HIDDEN: [&str; 4] = ["equals", "hashCode", "toString", "getClass"];

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
    let wildcard = u.types.intern(Type::Wildcard(Some(any_bounds)));
    let class_of_any = u.types.intern(Type::Applied(class, vec![wildcard]));
    method(u, any, "getClass", &[], class_of_any, &[FINAL]);
    poly(u, any, "isInstanceOf", any_bounds, |_, _| boolean);
    poly(u, any, "asInstanceOf", any_bounds, |u, x0| u.types.intern(x0.clone()));
    let hidden: Vec<SymbolId> = u.symbol(object).decls.clone();
    let kept: Vec<SymbolId> =
        hidden.into_iter().filter(|&d| !HIDDEN.contains(&u.symbol(d).name.as_str())).collect();
    u.symbols[object.0 as usize].decls = kept;
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
fn class_type(u: &mut Universe, pkg: &str, name: &str) -> TypeId {
    let prefix = match u.package(pkg) {
        Some(p) => u.types.intern(Type::Package(p)),
        None => u.types.intern(Type::PackageName(pkg.into())),
    };
    u.types.intern(Type::TypeRef(prefix, name.into()))
}

fn method(
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
) {
    let binder = u.fresh_binder();
    let x0 = Type::Param(binder, 0);
    let result = result(u, &x0);
    let info = u.types.intern(Type::Poly { binder, params: vec![("X0".into(), bounds)], result });
    let s = synthetic(u, owner, name, Kind::Def, &[FINAL]);
    u.infos.insert(s, info);
}
