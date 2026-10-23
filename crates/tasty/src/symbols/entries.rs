//! The package clauses and definitions a top-level file enters, in preorder: what full decoding
//! enters and what a library image's symbol table holds.

use super::loaded::Decoded;
use super::{Flags, Kind};
use crate::tags::*;
use crate::{Ast, NameRef, NodeId, View, view};

#[derive(Clone, PartialEq, Debug)]
pub struct Entry {
    /// `None` for a package clause, whose `name` is the package's qualified name.
    pub kind: Option<Kind>,
    pub name: String,
    /// The index of the enclosing entry; `None` at the top level.
    pub owner: Option<u32>,
    pub flags: Flags,
    pub target: Option<String>,
    pub is_extern: bool,
    pub derived: bool,
    pub tree: NodeId,
}

impl Entry {
    /// The tag of the tree it is read from.
    pub fn tree_tag(&self) -> u8 {
        match self.kind {
            None | Some(Kind::Package) => PACKAGE,
            Some(Kind::Class | Kind::Type) => TYPEDEF,
            Some(Kind::TypeParam) => TYPEPARAM,
            Some(Kind::Param) => PARAM,
            Some(Kind::Val) => VALDEF,
            Some(Kind::Def) => DEFDEF,
        }
    }
}

/// The entries of decoded file `file`; method bodies are not entered.
pub fn entries(file: &Decoded) -> Vec<Entry> {
    let mut out = Vec::new();
    for &root in file.ast.roots() {
        walk(file, root, None, &mut out);
    }
    out
}

fn walk(file: &Decoded, id: NodeId, owner: Option<u32>, out: &mut Vec<Entry>) {
    let (ast, names) = (&file.ast, file.names.as_slice());
    let name_of = |r: NameRef| names.get(r.0 as usize).cloned().unwrap_or_default();
    let at = Some(out.len() as u32);
    let (kind, name, mods, members): (Kind, _, &[NodeId], &[NodeId]) = match view(ast, id) {
        View::Package { pid, stats } => {
            let name = name_of(NameRef(ast.node(ast.deref(pid)).a as u32));
            out.push(Entry {
                kind: None,
                name,
                owner,
                flags: Flags::default(),
                target: None,
                is_extern: false,
                derived: false,
                tree: id,
            });
            for &s in stats {
                walk(file, s, at, out);
            }
            return;
        }
        View::TypeDef { name, rhs, is_class: true, mods } => (Kind::Class, name, mods, ast.kids(rhs)),
        View::TypeDef { name, mods, .. } => (Kind::Type, name, mods, &[]),
        View::DefDef { name, parts } => (Kind::Def, name, parts.mods, parts.params),
        View::ValDef { name, mods, .. } => (Kind::Val, name, mods, &[]),
        View::Param { name, mods, .. } if ast.tag(id) == TYPEPARAM => (Kind::TypeParam, name, mods, &[]),
        View::Param { name, mods, .. } => (Kind::Param, name, mods, &[]),
        _ => return,
    };
    out.push(Entry {
        kind: Some(kind),
        name: name_of(name),
        owner,
        flags: mods.iter().fold(Flags::default(), |f, &m| f.with(ast.tag(m))),
        target: target_name(ast, names, mods),
        is_extern: annotations(ast, mods).any(|(ty, _)| names_ref(ast, names, ty, "extern")),
        derived: file.derived.get(name.0 as usize).copied().unwrap_or_default(),
        tree: id,
    });
    for &m in members {
        walk(file, m, at, out);
    }
}

/// Whether the tree at `id` is a reference named `name`.
pub fn names_ref(ast: &Ast, names: &[String], id: NodeId, name: &str) -> bool {
    let n = ast.node(id);
    matches!(n.tag, TYPEREF | TYPEREFin | SELECTtpt | IDENTtpt)
        && names.get(n.a as usize).is_some_and(|s| s == name)
}

/// The name a `@targetName("…")` annotation among modifiers `mods` gives.
fn target_name(ast: &Ast, names: &[String], mods: &[NodeId]) -> Option<String> {
    annotations(ast, mods).find_map(|(ty, call)| {
        mentions(ast, names, ty, "targetName").then(|| first_string(ast, names, call)).flatten()
    })
}

/// The annotations among modifiers `mods`: each one's type tree and constructor call.
pub fn annotations<'a>(ast: &'a Ast, mods: &'a [NodeId]) -> impl Iterator<Item = (NodeId, NodeId)> + 'a {
    mods.iter().filter(|&&m| ast.tag(m) == ANNOTATION).filter_map(|&m| match ast.kids(m) {
        [ty, call, ..] => Some((ast.deref(*ty), *call)),
        _ => None,
    })
}

/// Whether a reference in the tree at `id` names `name`.
fn mentions(ast: &Ast, names: &[String], id: NodeId, name: &str) -> bool {
    let n = ast.node(id);
    let named = matches!(n.tag, TYPEREF | TYPEREFin | SELECTtpt | IDENTtpt | TERMREF | SELECT | IDENT);
    (named && names.get(n.a as usize).is_some_and(|s| s == name))
        || ast.kids(id).iter().any(|&k| mentions(ast, names, ast.deref(k), name))
}

/// The first `Int` constant in the tree at `id`.
pub fn first_int(ast: &Ast, id: NodeId) -> Option<i32> {
    match ast.node(id) {
        n if n.tag == INTconst => Some(n.a as i32),
        _ => ast.kids(id).iter().find_map(|&k| first_int(ast, ast.deref(k))),
    }
}

/// The first string constant in the tree at `id`.
fn first_string(ast: &Ast, names: &[String], id: NodeId) -> Option<String> {
    match ast.node(id) {
        n if n.tag == STRINGconst => names.get(n.a as usize).cloned(),
        _ => ast.kids(id).iter().find_map(|&k| first_string(ast, names, k)),
    }
}
