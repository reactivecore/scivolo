//! The top-level imports of an input as source text for later inputs, a path that starts at a
//! definition of an input qualified by its wrapper object: `import rs$line$1.s.length`.

use crate::line_names::PREFIX as WRAPPER_PREFIX;
use syntax::source::SourceFile;
use tasty::{Kind, Universe};
use typer::Typed;
use typer::tpd::{TNodeId, Tree};

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
