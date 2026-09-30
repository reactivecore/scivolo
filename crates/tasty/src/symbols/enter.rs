//! Entering the symbols a top-level file defines: its packages' classes, types and members, and
//! recursively their members; method bodies are not entered.

use super::{Error, Flags, Kind, Loaded, Symbol, SymbolId, Universe};
use crate::tags::*;
use crate::{Ast, NodeId, Tasty, View, view};

impl Universe {
    /// Decodes the file at `path` and enters its symbols.
    pub(super) fn load(&mut self, path: &str) -> Result<(), Error> {
        let corrupt = |e| Error::Corrupt(path.to_owned(), e);
        let Some(bytes) = self.library.read(path) else {
            return Err(Error::Io(std::io::Error::other(format!("{path}: unreadable"))));
        };
        let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let tasty = Tasty::parse(bytes).map_err(corrupt)?;
        let Some(section) = tasty.section("ASTs") else { return Err(corrupt(missing(path))) };
        let ast = Ast::parse(section.bytes).map_err(corrupt)?;
        let file = self.files.len() as u32;
        let roots = ast.roots().to_vec();
        self.files.push(Loaded { tasty, ast });
        for root in roots {
            self.enter_stat(file, root, self.root());
        }
        Ok(())
    }

    /// Enters the definition or package at `id` of `file` into `owner`.
    fn enter_stat(&mut self, file: u32, id: NodeId, owner: SymbolId) {
        let loaded = &self.files[file as usize];
        let (ast, names) = (&loaded.ast, &loaded.tasty.names);
        let (kind, name, mods, members): (Kind, String, Vec<NodeId>, Vec<NodeId>) = match view(ast, id) {
            View::Package { pid, stats } => {
                let (name, stats) = (names.show(crate::NameRef(ast.node(pid).a as u32)), stats.to_vec());
                let pkg = self.package_entered(&name);
                for s in stats {
                    self.enter_stat(file, s, pkg);
                }
                return;
            }
            View::TypeDef { name, rhs, is_class: true, mods } => {
                let template = ast.kids(rhs).to_vec();
                (Kind::Class, names.show(name), mods.to_vec(), template)
            }
            View::TypeDef { name, mods, .. } => (Kind::Type, names.show(name), mods.to_vec(), Vec::new()),
            View::DefDef { name, parts } => (Kind::Def, names.show(name), parts.mods.to_vec(), Vec::new()),
            View::ValDef { name, mods, .. } => (Kind::Val, names.show(name), mods.to_vec(), Vec::new()),
            View::Param { name, mods, .. } if matches!(ast.tag(id), TYPEPARAM) => {
                (Kind::TypeParam, names.show(name), mods.to_vec(), Vec::new())
            }
            View::Param { name, mods, .. } => (Kind::Val, names.show(name), mods.to_vec(), Vec::new()),
            _ => return,
        };
        let flags = flags(ast, &mods);
        let sym = self.add(Symbol {
            name,
            kind,
            owner: Some(owner),
            flags,
            decls: Vec::new(),
            tree: Some((file, id)),
        });
        for m in members {
            self.enter_stat(file, m, sym);
        }
    }
}

/// The flags of modifier trees `mods`.
fn flags(ast: &Ast, mods: &[NodeId]) -> Flags {
    Flags(mods.iter().map(|&m| ast.tag(m)).filter(|&t| t < 64).fold(0, |bits, t| bits | 1 << t))
}

fn missing(path: &str) -> crate::Error {
    crate::Error { pos: 0, msg: format!("{path}: no ASTs section") }
}
