// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SpecializeApplyMethods.scala
//! The Java interfaces of functions that need no boxing, defined if the library has no TASTy for
//! them: `scala.runtime.function.JProcedureN` returning `Unit`, and the specialized
//! `scala.runtime.java8.JFunctionN$mc<R><Ps>$sp`; and the specialized `apply` methods of
//! `FunctionN` (dotty's `SpecializeApplyMethods`).

use super::{member, package, symbol, type_ref};
use tasty::tags::TRAIT;
use tasty::{Erased, Flags, Kind, SymbolId, Type, TypeId, Universe};

/// `JProcedure{arity}`, with abstract method `applyVoid` of object parameters.
pub fn procedure(u: &mut Universe, arity: usize) -> Result<SymbolId, String> {
    let object = type_ref(u, u.builtins().object);
    let unit = primitive(u, &Erased::Unit)?;
    let params = vec![object; arity];
    interface(u, "scala.runtime.function", &format!("JProcedure{arity}"), "applyVoid", &params, unit)
}

/// `JFunction{n}$mc<R><Ps>$sp` for erased primitive parameters `params` and result `result`,
/// with abstract method `apply$mc<R><Ps>$sp`.
pub fn specialized(u: &mut Universe, params: &[Erased], result: &Erased) -> Result<SymbolId, String> {
    let method = specialized_name(params, result);
    let name = format!("JFunction{}{}", params.len(), &method["apply".len()..]);
    let types = params.iter().map(|p| primitive(u, p)).collect::<Result<Vec<_>, _>>()?;
    let result = primitive(u, result)?;
    interface(u, "scala.runtime.java8", &name, &method, &types, result)
}

/// The specialized `apply$mc<R><Ps>$sp` method of `FunctionN` for erased primitive `params` and
/// `result`; entered if missing.
pub fn specialized_apply(u: &mut Universe, params: &[Erased], result: &Erased) -> Result<SymbolId, String> {
    let scala = package(u, "scala")?;
    let function = member(u, scala, &format!("Function{}", params.len()), Kind::Class)?;
    let name = specialized_name(params, result);
    if let Ok(m) = member(u, function, &name, Kind::Def) {
        return Ok(m);
    }
    let types = params.iter().map(|p| primitive(u, p)).collect::<Result<Vec<_>, _>>()?;
    let result = primitive(u, result)?;
    let m = u.define(symbol(&name, Kind::Def, function, Flags::default()));
    let info = super::equality::method(u, &types, result);
    u.set_info(m, info);
    Ok(m)
}

/// All specialized `apply` methods of `Function{arity}`, entered if missing.
pub fn specialized_applies(u: &mut Universe, arity: usize) -> Result<Vec<SymbolId>, String> {
    let prims = |s: &str| s.chars().map(Erased::Prim).collect::<Vec<_>>();
    let (results, params): (Vec<Erased>, Vec<Vec<Erased>>) = match arity {
        0 => {
            let results = prims("bscijfd").into_iter().chain([Erased::Unit, Erased::Prim('z')]);
            (results.collect(), vec![Vec::new()])
        }
        1 => (function_results(), prims("ijfd").into_iter().map(|p| vec![p]).collect()),
        _ => {
            let ps = prims("ijd");
            let params = ps.iter().flat_map(|p| ps.iter().map(|q| vec![p.clone(), q.clone()]));
            (function_results(), params.collect())
        }
    };
    let mut out = Vec::new();
    for r in &results {
        for ps in &params {
            out.push(specialized_apply(u, ps, r)?);
        }
    }
    Ok(out)
}

/// The results of the specialized `apply` methods of `Function1` and `Function2`.
fn function_results() -> Vec<Erased> {
    std::iter::once(Erased::Unit).chain("zifjd".chars().map(Erased::Prim)).collect()
}

/// `apply$mc<R><Ps>$sp`, each type by its Java descriptor letter.
fn specialized_name(params: &[Erased], result: &Erased) -> String {
    let letter = |e: &Erased| match e {
        Erased::Unit => 'V',
        Erased::Prim(c) => c.to_ascii_uppercase(),
        _ => unreachable!("a primitive type"),
    };
    let params: String = params.iter().map(letter).collect();
    format!("apply$mc{}{params}$sp", letter(result))
}

/// The class type of erased primitive `e`.
fn primitive(u: &mut Universe, e: &Erased) -> Result<TypeId, String> {
    let name = match e {
        Erased::Unit => "Unit",
        Erased::Prim('z') => "Boolean",
        Erased::Prim('b') => "Byte",
        Erased::Prim('s') => "Short",
        Erased::Prim('c') => "Char",
        Erased::Prim('i') => "Int",
        Erased::Prim('j') => "Long",
        Erased::Prim('f') => "Float",
        Erased::Prim('d') => "Double",
        _ => return Err("a primitive type".into()),
    };
    let scala = package(u, "scala")?;
    let class = member(u, scala, name, Kind::Class)?;
    Ok(type_ref(u, class))
}

/// Java interface `pkg.name`, a `FunctionN` with abstract method `method` and a default `apply`
/// of objects; defined if missing.
fn interface(
    u: &mut Universe,
    pkg: &str,
    name: &str,
    method_name: &str,
    params: &[TypeId],
    result: TypeId,
) -> Result<SymbolId, String> {
    let pkg = u.enter_package(pkg);
    if let Ok(c) = member(u, pkg, name, Kind::Class) {
        return Ok(c);
    }
    let scala = package(u, "scala")?;
    let function = member(u, scala, &format!("Function{}", params.len()), Kind::Class)?;
    let java_io = package(u, "java.io")?;
    let serializable = member(u, java_io, "Serializable", Kind::Class)?;
    let class = u.define(symbol(name, Kind::Class, pkg, Flags::default().with(TRAIT)));
    let parents = vec![type_ref(u, function), type_ref(u, serializable)];
    u.set_parents(class, parents);
    let abstract_ = method(u, class, method_name, params, result);
    u.set_deferred(abstract_, true);
    let object = type_ref(u, u.builtins().object);
    method(u, class, "apply", &vec![object; params.len()], object);
    Ok(class)
}

/// Method `name` of `owner`.
fn method(u: &mut Universe, owner: SymbolId, name: &str, params: &[TypeId], result: TypeId) -> SymbolId {
    let m = u.define(symbol(name, Kind::Def, owner, Flags::default()));
    let params = params.iter().enumerate().map(|(i, &t)| (format!("v{}", i + 1), t)).collect();
    let binder = u.fresh_binder();
    let info = u.intern_type(Type::Method { binder, params, result, implicit: false, contextual: false });
    u.set_info(m, info);
    m
}
