//! Entering the symbols a top-level file defines: its packages' classes, types and members, and
//! recursively their members; method bodies are not entered.

use super::{Error, FileId, Flags, Kind, Loaded, Symbol, SymbolId, Universe};
use crate::tags::*;
use crate::{Ast, NameRef, Names, NodeId, Tasty, View, view};

/// What entering reads: a file's trees and names.
struct Source<'s> {
    file: FileId,
    ast: &'s Ast,
    names: &'s Names<'s>,
}

impl Universe {
    /// Decodes the file at `path` and enters its symbols.
    pub fn load(&mut self, path: &str) -> Result<(), Error> {
        let bytes = self.library.read(path).map_err(|e| Error::Io(path.to_owned(), e))?.into_boxed_slice();
        let corrupt = |e| Error::Corrupt(path.to_owned(), e);
        let tasty = Tasty::parse(&bytes).map_err(corrupt)?;
        let Some(section) = tasty.section("ASTs") else {
            return Err(corrupt(crate::Error { pos: 0, msg: "no ASTs section".into() }));
        };
        let ast = Ast::parse(section.bytes).map_err(corrupt)?;
        let src = Source { file: FileId(self.files.len() as u32), ast: &ast, names: &tasty.names };
        for &root in ast.roots() {
            self.enter_stat(&src, root, self.root());
        }
        drop(tasty);
        self.files.push(Loaded { bytes, ast });
        Ok(())
    }

    /// Enters the definition or package at `id` into `owner`.
    fn enter_stat(&mut self, src: &Source, id: NodeId, owner: SymbolId) {
        let (ast, names) = (src.ast, src.names);
        let (kind, name, mods, members): (Kind, NameRef, &[NodeId], &[NodeId]) = match view(ast, id) {
            View::Package { pid, stats } => {
                let name = names.show(NameRef(ast.node(ast.deref(pid)).a as u32));
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
            name: names.show(name),
            kind,
            owner: Some(owner),
            flags,
            decls: Vec::new(),
            tree: Some((src.file, id)),
        });
        for &m in members {
            self.enter_stat(src, m, sym);
        }
    }
}
