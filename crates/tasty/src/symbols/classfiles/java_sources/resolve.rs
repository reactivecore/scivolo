// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! The class a name in a Java source stands for (JLS 6.5.5 as dotty's `typedIdent` and
//! `typedSelect` do it in Java units): type variables, member classes of the enclosing classes and
//! their supertypes, single-type imports, the package, on-demand imports and `java.lang`.

use super::super::classpath::java_class;
use super::syntax::{Class, Type};
use super::{JavaSources, Located};
use crate::symbols::{Kind, Universe};

/// How deep the supertypes of a class are searched for a member class.
const MAX_SUPERTYPE_DEPTH: usize = 32;

/// What a name stands for: a type variable, or a class by internal name.
pub enum Resolved {
    Var(String),
    Class(String),
}

/// The scope of a name in a Java source: the class it is in, the type variables in scope, and
/// whether the class's own member classes are left out, as in its `extends` and `implements`.
pub struct Scope<'a> {
    pub at: &'a Located,
    pub vars: Vec<String>,
    pub in_header: bool,
}

/// Determines what the name `parts` (without type arguments) stands for in `scope`; the refusal
/// of a name found nowhere.
pub fn resolve(
    u: &mut Universe,
    sources: &JavaSources,
    scope: &Scope,
    parts: &[&str],
) -> Result<Resolved, String> {
    if let [name] = parts
        && scope.vars.iter().any(|v| v == name)
    {
        return Ok(Resolved::Var((*name).to_owned()));
    }
    let class = match simple(u, sources, scope, parts[0]) {
        Some(class) => nested_path(u, sources, class, &parts[1..]),
        None => package_path(u, sources, parts),
    };
    class.map(Resolved::Class).ok_or_else(|| format!("Java type `{}` not found", parts.join(".")))
}

/// The class `internal` then its member classes `rest`, each found as `member_class` finds it.
fn nested_path(u: &mut Universe, sources: &JavaSources, internal: String, rest: &[&str]) -> Option<String> {
    rest.iter().try_fold(internal, |c, name| member_class(u, sources, &c, name, 0))
}

/// The class simple name `name` stands for in `scope`, a type variable aside.
fn simple(u: &mut Universe, sources: &JavaSources, scope: &Scope, name: &str) -> Option<String> {
    let unit = &scope.at.unit;
    for outer in scope.at.enclosing().into_iter().skip(usize::from(scope.in_header)) {
        if let Some(c) = member_class(u, sources, &outer, name, 0) {
            return Some(c);
        }
    }
    let top = unit.classes.iter().find(|c| c.name == name);
    let package = unit.package.replace('.', "/");
    let in_package = |n: &str| if package.is_empty() { n.to_owned() } else { format!("{package}/{n}") };
    if top.is_some() {
        return Some(in_package(name));
    }
    for i in unit.imports.iter().filter(|i| !i.on_demand && i.path.last().is_some_and(|l| l == name)) {
        let parts: Vec<&str> = i.path.iter().map(String::as_str).collect();
        if let Some(c) = package_path(u, sources, &parts) {
            return Some(c);
        }
    }
    if exists(u, sources, &in_package(name)) {
        return Some(in_package(name));
    }
    let on_demand = unit.imports.iter().filter(|i| i.on_demand).map(|i| i.path.join("/"));
    let on_demand: Vec<String> = on_demand.chain(std::iter::once("java/lang".to_owned())).collect();
    on_demand.into_iter().find_map(|p| {
        let internal = format!("{p}/{name}");
        if exists(u, sources, &internal) { Some(internal) } else { member_class(u, sources, &p, name, 0) }
    })
}

/// The class name `parts` stands for as a package's class then its member classes, the shortest
/// package first.
fn package_path(u: &mut Universe, sources: &JavaSources, parts: &[&str]) -> Option<String> {
    (1..parts.len()).find_map(|i| {
        let internal = format!("{}/{}", parts[..i].join("/"), parts[i]);
        exists(u, sources, &internal).then(|| nested_path(u, sources, internal, &parts[i + 1..]))?
    })
}

/// The member class `name` of class `owner` (internal names), declared in it or inherited from
/// its supertypes.
fn member_class(
    u: &mut Universe,
    sources: &JavaSources,
    owner: &str,
    name: &str,
    depth: usize,
) -> Option<String> {
    if depth > MAX_SUPERTYPE_DEPTH {
        return None;
    }
    let candidate = format!("{owner}${name}");
    if exists(u, sources, &candidate) {
        return Some(candidate);
    }
    supertypes(u, sources, owner).into_iter().find_map(|s| member_class(u, sources, &s, name, depth + 1))
}

/// The internal names of the direct supertypes of class `internal`; those of a source class
/// resolved in its own scope, those it cannot resolve left out.
fn supertypes(u: &mut Universe, sources: &JavaSources, internal: &str) -> Vec<String> {
    if let Some(at) = sources.located(internal) {
        let class: &Class = at.class();
        let scope = Scope { at: &at, vars: Vec::new(), in_header: true };
        let parents = class.extends.iter().chain(&class.implements);
        let named = parents.filter_map(|t| if let Type::Named(parts) = t { Some(parts) } else { None });
        return named
            .filter_map(|parts| {
                let names: Vec<&str> = parts.iter().map(|(n, _)| n.as_str()).collect();
                match resolve(u, sources, &scope, &names) {
                    Ok(Resolved::Class(c)) => Some(c),
                    _ => None,
                }
            })
            .collect();
    }
    let info = java_class(u, internal).or_else(|| u.jdk.as_mut()?.class(internal).cloned());
    info.map(|i| i.superclass.iter().chain(&i.interfaces).cloned().collect()).unwrap_or_default()
}

/// Determines whether class `internal` exists: a Java source's, a jar's, the JDK's, or a
/// package's class the universe knows, as a Scala source of the run defines it.
pub fn exists(u: &mut Universe, sources: &JavaSources, internal: &str) -> bool {
    if sources.located(internal).is_some() {
        return true;
    }
    if u.java_classes.as_ref().is_some_and(|c| c.in_jars(internal)) {
        return true;
    }
    if u.jdk.as_mut().is_some_and(|jdk| jdk.class(internal).is_some()) {
        return true;
    }
    let Some((package, name)) = internal.rsplit_once('/') else { return false };
    if name.contains('$') {
        return false;
    }
    let Some(p) = u.package(&package.replace('/', ".")) else { return false };
    u.decls_named(p, name).into_iter().any(|d| u.symbol(d).kind == Kind::Class && !u.is_module_class(d))
}
