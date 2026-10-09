//! Entering the symbols a top-level file defines: its packages' classes, types and members, and
//! recursively their members; method bodies are not entered.

use super::{Error, FileId, Flags, Kind, Loaded, Symbol, SymbolId, Universe};
use crate::tags::*;
use crate::{Ast, NodeId, Tasty, View, view};

/// What entering reads: a file's trees and names.
struct Source<'s> {
    file: FileId,
    ast: &'s Ast,
    names: &'s [String],
    /// Whether each name is of a kind other than `Utf8`.
    derived: &'s [bool],
}

impl Universe {
    /// Decodes the file at `path` and enters its symbols.
    pub fn load(&mut self, path: &str) -> Result<(), Error> {
        let bytes = self.library.read(path).map_err(|e| Error::Io(path.to_owned(), e))?;
        let corrupt = |e| Error::Corrupt(path.to_owned(), e);
        let tasty = Tasty::parse(&bytes).map_err(corrupt)?;
        let Some(section) = tasty.section("ASTs") else {
            return Err(corrupt(crate::Error { pos: 0, msg: "no ASTs section".into() }));
        };
        let ast = Ast::parse(section.bytes).map_err(corrupt)?;
        let names: Vec<String> = tasty.names.iter().map(|(r, _)| tasty.names.show(r)).collect();
        let attributes = tasty.attributes().map_err(corrupt)?;
        let scala2 = attributes.iter().any(|a| matches!(a, crate::Attribute::Flag(1)));
        let derived: Vec<bool> =
            tasty.names.iter().map(|(_, n)| !matches!(n, crate::Name::Utf8(_))).collect();
        let src =
            Source { file: FileId(self.files.len() as u32), ast: &ast, names: &names, derived: &derived };
        for &root in ast.roots() {
            self.enter_stat(&src, root, self.root());
        }
        self.files.push(Loaded { ast, names, scala2 });
        Ok(())
    }

    /// Enters the definition or package at `id` into `owner`.
    fn enter_stat(&mut self, src: &Source, id: NodeId, owner: SymbolId) {
        let (ast, names) = (src.ast, src.names);
        let name_of = |r: crate::NameRef| names.get(r.0 as usize).cloned().unwrap_or_default();
        let (kind, name, mods, members): (Kind, _, &[NodeId], &[NodeId]) = match view(ast, id) {
            View::Package { pid, stats } => {
                let name = name_of(crate::NameRef(ast.node(ast.deref(pid)).a as u32));
                let pkg = self.package_entered(&name);
                for &s in stats {
                    self.enter_stat(src, s, pkg);
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
        let flags = mods.iter().fold(Flags::default(), |f, &m| f.with(ast.tag(m)));
        let derived = src.derived.get(name.0 as usize).copied().unwrap_or_default();
        let (owner, name) = self.jdk_renamed(owner, name_of(name));
        let sym = self.add(Symbol {
            name,
            kind,
            owner: Some(owner),
            flags,
            target: target_name(ast, names, mods),
            is_extern: annotations(ast, mods).any(|(ty, _)| names_ref(ast, names, ty, "extern")),
            derived,
            decls: Vec::new(),
            tree: Some((src.file, id)),
        });
        for &m in members {
            self.enter_stat(src, m, sym);
        }
    }

    /// The owner and name of a definition named `name` in `owner`: its JDK package and name if
    /// Scala Native defines it under another name.
    fn jdk_renamed(&mut self, owner: SymbolId, name: String) -> (SymbolId, String) {
        if !name.starts_with('_') {
            return (owner, name);
        }
        if self.symbol(owner).kind != Kind::Package {
            let renamed = (self.symbol(owner).name == "Object")
                .then(|| super::jdk::jdk_member(&self.full_name(owner), &name))
                .flatten();
            return (owner, renamed.unwrap_or(name));
        }
        match super::jdk::jdk_name(&self.full_name(owner), &name) {
            Some((pkg, jdk)) => (self.package_entered(pkg), jdk),
            None => (owner, name),
        }
    }
}

/// Whether the tree at `id` is a reference named `name`.
fn names_ref(ast: &Ast, names: &[String], id: NodeId, name: &str) -> bool {
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
fn annotations<'a>(ast: &'a Ast, mods: &'a [NodeId]) -> impl Iterator<Item = (NodeId, NodeId)> + 'a {
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

/// The first string constant in the tree at `id`.
fn first_string(ast: &Ast, names: &[String], id: NodeId) -> Option<String> {
    match ast.node(id) {
        n if n.tag == STRINGconst => names.get(n.a as usize).cloned(),
        _ => ast.kids(id).iter().find_map(|&k| first_string(ast, names, k)),
    }
}
