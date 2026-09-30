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
        let src = Source { file: FileId(self.files.len() as u32), ast: &ast, names: &names };
        for &root in ast.roots() {
            self.enter_stat(&src, root, self.root());
        }
        self.files.push(Loaded { ast, names });
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
        let sym = self.add(Symbol {
            name: name_of(name),
            kind,
            owner: Some(owner),
            flags,
            target: target_name(ast, names, mods),
            decls: Vec::new(),
            tree: Some((src.file, id)),
        });
        for &m in members {
            self.enter_stat(src, m, sym);
        }
    }
}

/// The name a `@targetName("…")` annotation among modifiers `mods` gives.
fn target_name(ast: &Ast, names: &[String], mods: &[NodeId]) -> Option<String> {
    mods.iter().filter(|&&m| ast.tag(m) == ANNOTATION).find_map(|&m| {
        let [ty, call, ..] = ast.kids(m) else { return None };
        mentions(ast, names, *ty, "targetName").then(|| first_string(ast, names, *call)).flatten()
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
