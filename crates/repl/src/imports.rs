//! The top-level imports of an input: as source text for later inputs, a path that starts at a
//! definition of an input qualified by its wrapper object (`import rs$line$1.s.length`); as
//! `:imports` lists them (dotty's `RefinedPrinter.importText` of the typed import).

use crate::line_names::PREFIX as WRAPPER_PREFIX;
use syntax::names::Names;
use syntax::source::SourceFile;
use tasty::tags::OBJECT;
use tasty::{Kind, SymbolId, Universe};
use typer::Typed;
use typer::tpd::{TNodeId, Tree};

/// The owners whose members the root imports bring, so that a path to them names no owner.
const ROOT_IMPORTED: [&str; 3] = ["scala", "java.lang", "scala.Predef"];

/// The imports in wrapper template `template` of `typed`, as `:imports` lists them.
pub fn shown(
    u: &Universe,
    typed: &Typed,
    template: TNodeId,
    names: &Names,
    source: &SourceFile,
) -> Vec<String> {
    let Tree::Template { body, .. } = *typed.tpd.tree(template) else { return Vec::new() };
    let shown = |&s: &TNodeId| {
        let Tree::Import { expr, selectors } = *typed.tpd.tree(s) else { return None };
        let selectors: Vec<(String, bool)> =
            typed.tpd.items(selectors).iter().map(|&s| selector(typed, names, s)).collect();
        let selectors = match &selectors[..] {
            [(text, true)] => text.clone(),
            _ => format!("{{{}}}", selectors.into_iter().map(|(t, _)| t).collect::<Vec<_>>().join(", ")),
        };
        Some(format!("import {}.{selectors}", path(u, typed, names, source, expr)))
    };
    typed.tpd.items(body).iter().filter_map(shown).collect()
}

/// The text of import selector `s`, and whether it is a name or wildcard alone.
fn selector(typed: &Typed, names: &Names, s: TNodeId) -> (String, bool) {
    let name = |t: TNodeId| match *typed.tpd.tree(t) {
        Tree::Ident { name } => names.text(name.name),
        _ => "<error>",
    };
    let Tree::ImportSelector { imported, renamed, .. } = *typed.tpd.tree(s) else {
        return ("<error>".into(), false);
    };
    let imported = match name(imported) {
        "_" => "*",
        "" => "given",
        "*" => "`*`",
        n => n,
    };
    match renamed {
        Some(r) => (format!("{imported} as {}", name(r)), false),
        None => (imported.to_owned(), true),
    }
}

/// The text of import path `t`: a selection as written, an identifier through the owners of
/// its symbol unless a root import brings it.
fn path(u: &Universe, typed: &Typed, names: &Names, source: &SourceFile, t: TNodeId) -> String {
    let span = typed.tpd.span(t);
    let text = source.text.get(span.start() as usize..span.end() as usize).unwrap_or_default();
    match *typed.tpd.tree(t) {
        Tree::Select { qualifier, name } => {
            format!("{}.{}", path(u, typed, names, source, qualifier), names.text(name.name))
        }
        Tree::Ident { name } if text.starts_with('`') => format!("`{}`", names.text(name.name)),
        Tree::Ident { name } => match typed.tpd.sym(t) {
            Some(sym) => symbol_path(u, sym),
            None => names.text(name.name).to_owned(),
        },
        _ => text.to_owned(),
    }
}

/// `sym` through its owners, without package objects, the root and empty packages and wrapper
/// objects.
fn symbol_path(u: &Universe, sym: SymbolId) -> String {
    let simple = |s: SymbolId| {
        let s = u.symbol(s);
        let module_class = s.kind == Kind::Class && s.flags.has(OBJECT);
        (if module_class { s.name.strip_suffix('$') } else { None }).unwrap_or(&s.name).to_owned()
    };
    let mut owners = Vec::new();
    let mut owner = u.symbol(sym).owner;
    while let Some(o) = owner {
        let name = simple(o);
        if !(matches!(name.as_str(), "" | "<root>" | "_root_" | "package") || name.ends_with("$package")) {
            owners.push(name);
        }
        owner = u.symbol(o).owner;
    }
    owners.reverse();
    if ROOT_IMPORTED.contains(&owners.join(".").as_str()) {
        return simple(sym);
    }
    owners.retain(|o| o != "<empty>" && !o.starts_with(WRAPPER_PREFIX));
    owners.push(simple(sym));
    owners.join(".")
}

/// The imports in wrapper template `template` of `typed`, as source text with `import` in front.
pub fn imports(u: &Universe, typed: &Typed, template: TNodeId, source: &SourceFile) -> Vec<String> {
    let Tree::Template { body, .. } = *typed.tpd.tree(template) else { return Vec::new() };
    let stats = typed.tpd.items(body).to_vec();
    stats.into_iter().filter_map(|s| import(u, typed, s, source)).collect()
}

fn import(u: &Universe, typed: &Typed, t: TNodeId, source: &SourceFile) -> Option<String> {
    let Tree::Import { expr, .. } = *typed.tpd.tree(t) else { return None };
    let span = typed.tpd.span(t);
    let (start, end) = (span.start() as usize, span.end() as usize);
    let mut text = source.text.get(start..end)?.to_owned();
    if let Some((at, object)) = wrapper_root(u, typed, expr)
        && let Some(at) = (at as usize).checked_sub(start).filter(|&a| a <= text.len())
        && !text[at..].starts_with(WRAPPER_PREFIX)
    {
        text.insert_str(at, &format!("{object}."));
    }
    let first = text.strip_prefix("import").is_some_and(|r| r.starts_with(char::is_whitespace));
    Some(if first { text } else { format!("import {text}") })
}

/// Where path `t` names a member of a wrapper object first, and that object.
fn wrapper_root(u: &Universe, typed: &Typed, t: TNodeId) -> Option<(u32, String)> {
    let mut path = vec![t];
    while let Tree::Select { qualifier, .. } = *typed.tpd.tree(*path.last()?) {
        path.push(qualifier);
    }
    path.into_iter().rev().find_map(|n| {
        let owner = u.symbol(typed.tpd.sym(n)?).owner?;
        let s = u.symbol(owner);
        let object = s.name.strip_suffix('$').filter(|o| o.starts_with(WRAPPER_PREFIX));
        let span = typed.tpd.span(n);
        (s.kind == Kind::Class && span.exists()).then_some((span.start(), object?.to_owned()))
    })
}
