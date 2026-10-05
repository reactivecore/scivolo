use super::{member, package, symbol, type_ref};
use tasty::tags::TRAIT;
use tasty::{Flags, Kind, SymbolId, Type, Universe};

/// `scala.runtime.function.JProcedure{arity}`, the Java interface of the functions of `arity`
/// parameters returning `Unit`: a `FunctionN` with abstract method `applyVoid`; defined if the
/// library has no TASTy for it.
pub fn procedure(u: &mut Universe, arity: usize) -> Result<SymbolId, String> {
    let pkg = u.enter_package("scala.runtime.function");
    let name = format!("JProcedure{arity}");
    if let Ok(c) = member(u, pkg, &name, Kind::Class) {
        return Ok(c);
    }
    let scala = package(u, "scala")?;
    let function = member(u, scala, &format!("Function{arity}"), Kind::Class)?;
    let java_io = package(u, "java.io")?;
    let serializable = member(u, java_io, "Serializable", Kind::Class)?;
    let unit = member(u, scala, "Unit", Kind::Class)?;
    let class = u.define(symbol(&name, Kind::Class, pkg, Flags::default().with(TRAIT)));
    let parents = vec![type_ref(u, function), type_ref(u, serializable)];
    u.set_parents(class, parents);
    let object = type_ref(u, u.builtins().object);
    let params = (1..=arity).map(|i| (format!("t{i}"), object)).collect();
    let result = type_ref(u, unit);
    let apply = u.define(symbol("applyVoid", Kind::Def, class, Flags::default()));
    let binder = u.fresh_binder();
    let info = u.intern_type(Type::Method { binder, params, result, implicit: false });
    u.set_info(apply, info);
    Ok(class)
}
