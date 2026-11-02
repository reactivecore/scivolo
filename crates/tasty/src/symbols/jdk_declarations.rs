//! The JDK's declarations of classes javalib defines (`libs/jdk-stubs/JdkDeclarations.scala`),
//! entered over javalib's classes: each member they declare replaces javalib's of its name.

use super::entries::Entry;
use super::{FileId, Kind, Symbol, SymbolId, Universe};
use crate::tags::{OBJECT, SYNTHETIC};

/// The directory of the JDK's declarations in the library, beside the packages.
pub const JDK_DECLARATIONS: &str = "jdk-declarations/";

#[derive(Clone, Copy)]
enum Entered {
    /// javalib's symbol of the declaration.
    Merged(SymbolId),
    /// A symbol new to javalib's class.
    New(SymbolId),
    Skipped,
}

impl Universe {
    /// Loads the JDK's declarations of the classes of file `path` over its symbols, unless JDK
    /// jars stand for the JDK.
    pub fn load_jdk_declarations(&mut self, path: &str) -> Result<(), super::Error> {
        if !self.jdk_jars.is_empty() {
            return Ok(());
        }
        let declared = format!("{JDK_DECLARATIONS}{path}");
        match self.read_entries(&declared) {
            Ok((file, entries)) => {
                self.enter_declarations(file, entries);
                Ok(())
            }
            Err(super::Error::Io(_, e)) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Enters `entries` of file `file` over javalib's symbols: a class or object javalib has is
    /// merged into its, a type parameter into javalib's at its position, a member replaces
    /// javalib's of its name; constructors, synthetic members and what javalib lacks are skipped.
    fn enter_declarations(&mut self, file: FileId, entries: Vec<Entry>) {
        let mut ids: Vec<Entered> = Vec::with_capacity(entries.len());
        let mut type_params: Vec<SymbolId> = Vec::new();
        let mut replaced: Vec<(SymbolId, String)> = Vec::new();
        for e in entries {
            let owner = e.owner.map_or(Entered::Merged(self.root()), |o| ids[o as usize]);
            let entered = match (owner, e.kind) {
                (Entered::Skipped, _) => Entered::Skipped,
                (_, None) => Entered::Merged(self.enter_package(&e.name)),
                (Entered::New(owner), Some(kind)) => {
                    Entered::New(self.define_declared(owner, kind, file, &e))
                }
                (Entered::Merged(owner), Some(kind)) => match kind {
                    Kind::Class => self.javalib_decl(owner, &e.name, kind),
                    Kind::Val if e.flags.has(OBJECT) => self.javalib_decl(owner, &e.name, kind),
                    Kind::TypeParam => {
                        let n = type_params.iter().filter(|&&c| c == owner).count();
                        type_params.push(owner);
                        let params = self.symbol(owner).decls.iter().copied();
                        let mut params = params.filter(|&d| self.symbol(d).kind == Kind::TypeParam);
                        params.nth(n).map_or(Entered::Skipped, Entered::Merged)
                    }
                    Kind::Param => Entered::Skipped,
                    _ if e.name == "<init>" || e.flags.has(SYNTHETIC) => Entered::Skipped,
                    _ => {
                        if !replaced.iter().any(|(c, n)| *c == owner && *n == e.name) {
                            self.hide_decls(owner, &e.name);
                            replaced.push((owner, e.name.clone()));
                        }
                        Entered::New(self.define_declared(owner, kind, file, &e))
                    }
                },
            };
            if let Entered::Merged(s) = entered
                && e.kind.is_some()
            {
                self.by_tree.insert((file, e.tree), s);
            }
            ids.push(entered);
        }
    }

    /// javalib's declaration of `owner` named `name` of kind `kind`.
    fn javalib_decl(&self, owner: SymbolId, name: &str, kind: Kind) -> Entered {
        let decls = self.symbol(owner).decls.iter().copied();
        let mut named = decls.filter(|&d| self.symbol(d).name == name && self.symbol(d).kind == kind);
        named.next().map_or(Entered::Skipped, Entered::Merged)
    }

    /// Removes the terms and types `owner` declares named `name`.
    fn hide_decls(&mut self, owner: SymbolId, name: &str) {
        let decls = self.symbol(owner).decls.clone();
        for d in decls {
            let s = self.symbol(d);
            if s.name == name && !matches!(s.kind, Kind::Class | Kind::Package) {
                self.unenter(d);
            }
        }
    }

    fn define_declared(&mut self, owner: SymbolId, kind: Kind, file: FileId, e: &Entry) -> SymbolId {
        self.define(Symbol {
            name: e.name.clone(),
            kind,
            owner: Some(owner),
            flags: e.flags,
            target: e.target.clone(),
            is_extern: e.is_extern,
            derived: e.derived,
            decls: Vec::new(),
            tree: Some((file, e.tree)),
        })
    }
}
