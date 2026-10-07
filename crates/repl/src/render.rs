//! The definitions of an input as dotty's `Rendering` prints them: `val x: Int = 1`,
//! `def f(x: Int): Int`, `// defined class C`.

use crate::line_names::PREFIX as WRAPPER_PREFIX;
use crate::show_methods::SUFFIX;
use tasty::tags::{ARTIFACT, LAZY, OBJECT, PRIVATE, SYNTHETIC};
use tasty::{Erased, Kind, SymbolId, Universe};
use typer::Typed;
use typer::tpd::{TNodeId, Tree};
use vm::{Vm, VmError};

/// How many lines a value prints at most, as dotty's `-Xrepl-print-height`.
const HEIGHT: i32 = 50;

/// A definition to print: its text, and for a value the show method of its value.
pub struct Definition {
    text: String,
    show: Option<String>,
}

/// The definitions of module class `module` but `unprinted`, in source order.
pub fn definitions(u: &mut Universe, module: &str, unprinted: &[SymbolId]) -> Vec<Definition> {
    let Some(class) = module_class(u, module) else { return Vec::new() };
    let decls = u.symbol(class).decls.clone();
    decls.into_iter().filter(|d| !unprinted.contains(d)).filter_map(|d| definition(u, d)).collect()
}

/// The values of wrapper template `template` of `typed` whose inferred type is `Unit`.
pub fn inferred_units(u: &mut Universe, typed: &Typed, template: TNodeId) -> Vec<SymbolId> {
    let Tree::Template { body, .. } = *typed.tpd.tree(template) else { return Vec::new() };
    let mut units = Vec::new();
    for &s in typed.tpd.items(body) {
        let (Tree::ValDef { tpt, .. }, Some(sym)) = (typed.tpd.tree(s), typed.tpd.sym(s)) else { continue };
        if matches!(typed.tpd.tree(*tpt), Tree::TypeTree { inferred: true }) {
            let info = u.info(sym);
            if u.erase(info) == Erased::Unit {
                units.push(sym);
            }
        }
    }
    units
}

/// How many of `values` are `res` values.
pub fn results(u: &Universe, values: &[SymbolId]) -> u32 {
    values.iter().filter(|&&v| is_res(&u.symbol(v).name)).count() as u32
}

fn is_res(name: &str) -> bool {
    name.strip_prefix("res").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The class of top-level module class `module`.
pub fn module_class(u: &mut Universe, module: &str) -> Option<SymbolId> {
    let root = u.root();
    u.decl(root, module).unwrap_or_default().into_iter().find(|&c| u.symbol(c).kind == Kind::Class)
}

fn definition(u: &mut Universe, d: SymbolId) -> Option<Definition> {
    let s = u.symbol(d);
    let (name, kind, flags) = (s.name.clone(), s.kind, s.flags);
    let hidden = [PRIVATE, SYNTHETIC, ARTIFACT].iter().any(|&f| flags.has(f));
    let special = name.strip_prefix('<').is_some_and(|n| n.starts_with(char::is_alphabetic));
    let generated = name.ends_with("_=") || is_default_getter(&name);
    if hidden || special || generated || name.ends_with(SUFFIX) {
        return None;
    }
    let text = |text: String| Some(Definition { text, show: None });
    match kind {
        Kind::Class => text(format!("// defined {}", u.show_def(d))),
        Kind::Type if u.alias_of(d).is_some() => text(format!("// defined alias {}", u.show_def(d))),
        Kind::Type => text(format!("// defined {}", u.show_def(d))),
        Kind::Val if flags.has(OBJECT) => None,
        Kind::Val | Kind::Param if flags.has(LAZY) => text(u.show_def(d)),
        Kind::Val | Kind::Param => {
            let info = u.info(d);
            let show = Some(tasty::encode(&format!("{name}{SUFFIX}")));
            let text = match name.strip_suffix("$assign") {
                Some(var) => format!("{var}: {}", u.show_user(info)),
                None => u.show_def(d),
            };
            Some(Definition { text, show })
        }
        Kind::Def => text(u.show_def(d)),
        Kind::Package | Kind::TypeParam => None,
    }
}

/// Whether `name` has the form `m$default$N` (dotty's `DefaultGetterName`).
fn is_default_getter(name: &str) -> bool {
    name.rsplit_once("$default$").is_some_and(|(_, n)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The text that prints definition `d` of module `module` in `width` columns, running its show
/// method.
pub fn line(vm: &mut Vm, module: &str, d: &Definition, width: i32) -> Result<String, VmError> {
    let Some(method) = &d.show else { return Ok(d.text.clone()) };
    let prefix = format!("{} = ", d.text);
    let offset = prefix.lines().last().unwrap_or_default().encode_utf16().count() as i32;
    let shown = vm.show_value(module, method, [width, HEIGHT, offset])?;
    Ok(format!("{prefix}{}", strip_wrapper(&shown)))
}

/// `s` without a leading wrapper name `rs$line$N.` or `rs$line$N$` (dotty's `stripReplPrefix`).
fn strip_wrapper(s: &str) -> &str {
    match s.strip_prefix(WRAPPER_PREFIX) {
        Some(rest) => rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '$'),
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::strip_wrapper;

    #[test]
    fn strips_the_wrapper_name() {
        assert_eq!(strip_wrapper("rs$line$3$C@1f"), "C@1f");
        assert_eq!(strip_wrapper("List(rs$line$3$C@1f)"), "List(rs$line$3$C@1f)");
    }
}
