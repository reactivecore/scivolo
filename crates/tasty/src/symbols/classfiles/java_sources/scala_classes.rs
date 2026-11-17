//! The Scala classes a Java source names by internal name: `p/C`, a member class `p/C$D` of a
//! class or object, and an object's class `p/O$` (dotty's `typedIdent` and `typedSelect` in Java
//! units, which try the object and then the class as qualifier).

use crate::symbols::{Kind, SymbolId, Universe};
use crate::types::{Type, TypeId};

/// The Scala class of internal name `internal`, if the universe has one: a package's class, then
/// member classes of the classes or objects named by the parts between `$`, a trailing `$`
/// naming an object's class; none, without looking it up, if its top-level class is a Java
/// class of a jar, a Java source or the JDK.
pub fn scala_class(u: &mut Universe, internal: &str) -> Option<SymbolId> {
    let (package, simple) = internal.rsplit_once('/').unwrap_or(("", internal));
    let parts: Vec<&str> = simple.split('$').collect();
    let top = internal.split('$').next().unwrap_or(internal);
    let jars_or_sources = u.java_classes.as_ref().is_some_and(|c| c.declares(top));
    if jars_or_sources || u.jdk.as_mut().is_some_and(|jdk| jdk.class(top).is_some()) {
        return None;
    }
    let package = u.package(&package.replace('/', "."))?;
    member(u, package, &parts)
}

/// The class `parts` names in `owner`: `[C]` a class, `[O, ""]` an object's class, `[C, rest..]`
/// a member of class `C` or else of object `C`.
fn member(u: &mut Universe, owner: SymbolId, parts: &[&str]) -> Option<SymbolId> {
    match parts {
        [name] if !name.is_empty() => class(u, owner, name, false),
        [name, ""] => class(u, owner, name, true),
        [name, rest @ ..] if !name.is_empty() => [false, true].into_iter().find_map(|object| {
            let c = class(u, owner, name, object)?;
            member(u, c, rest)
        }),
        _ => None,
    }
}

/// The class `name` of `owner`, or with `object` the class of its object `name`.
fn class(u: &mut Universe, owner: SymbolId, name: &str, object: bool) -> Option<SymbolId> {
    let name = if object { format!("{name}$") } else { name.to_owned() };
    let decls = u.decls_named(owner, name.as_str());
    decls.into_iter().find(|&d| u.symbol(d).kind == Kind::Class && u.is_module_class(d) == object)
}

/// A reference to Scala class `class` as a Java source names it: through its package's `this`,
/// a member of an object through the object, of a class by projection (`C#D`).
pub fn scala_class_ref(u: &mut Universe, class: SymbolId) -> TypeId {
    let owner = u.symbol(class).owner.expect("a class has an owner");
    let prefix = prefix(u, owner);
    let name = u.symbol(class).name.clone();
    let name = u.types.intern_name(&name);
    u.types.intern(Type::TypeRef(prefix, name))
}

/// The prefix of the members of `owner`: a package's `this`, an object's reference, a class's
/// type.
fn prefix(u: &mut Universe, owner: SymbolId) -> TypeId {
    if u.symbol(owner).kind == Kind::Package {
        let package = u.types.intern(Type::Package(owner));
        return u.types.intern(Type::This(package));
    }
    if !u.is_module_class(owner) {
        return scala_class_ref(u, owner);
    }
    let outer = u.symbol(owner).owner.expect("an object's class has an owner");
    let outer = prefix(u, outer);
    let object = u.symbol(owner).name.trim_end_matches('$').to_owned();
    let object = u.types.intern_name(&object);
    u.types.intern(Type::TermRef(outer, object))
}
