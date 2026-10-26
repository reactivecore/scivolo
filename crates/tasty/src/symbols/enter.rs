//! Entering the symbols a top-level file defines: from the library image's symbol table of the
//! file if it has one, else from the decoded file.

use super::entries::{Entry, annotations, entries, first_int, first_string, names_ref};
use super::loaded::{Decoded, Loaded};
use super::{Error, FileId, Kind, Symbol, SymbolId, Universe, table};
use crate::{NodeId, View, view};

impl Universe {
    /// Loads the file at `path` and enters its symbols.
    pub fn load(&mut self, path: &str) -> Result<(), Error> {
        let (bytes, table) = self.library.read(path).map_err(|e| Error::Io(path.to_owned(), e))?;
        let (loaded, entries) = match table.and_then(table::decode) {
            Some((scala2, entries)) => {
                let entered = entries.iter().map(|e| (e.tree, e.tree_tag())).collect();
                (Loaded::undecoded(path, bytes, scala2, entered), entries)
            }
            None => {
                let decoded = Decoded::parse(&bytes).map_err(|e| Error::Corrupt(path.to_owned(), e))?;
                let entries = entries(&decoded);
                (Loaded::decoded_now(path, bytes, decoded), entries)
            }
        };
        let file = FileId(self.files.len() as u32);
        self.files.push(loaded);
        self.enter_entries(file, entries);
        Ok(())
    }

    /// Loads the TASTy of a pickled quote, which enters no symbols; its trees are the quote's.
    pub fn load_quote(&mut self, bytes: Vec<u8>) -> Result<FileId, Error> {
        let path = "<quote>";
        let decoded = Decoded::parse(&bytes).map_err(|e| Error::Corrupt(path.to_owned(), e))?;
        let file = FileId(self.files.len() as u32);
        self.files.push(Loaded::decoded_now(path, bytes.into(), decoded));
        Ok(file)
    }

    /// Enters `entries` of loaded file `file`, each into the symbol of its owner entry.
    fn enter_entries(&mut self, file: FileId, entries: Vec<Entry>) {
        let mut ids: Vec<SymbolId> = Vec::with_capacity(entries.len());
        for e in entries {
            let owner = e.owner.map_or(self.root(), |o| ids[o as usize]);
            let Some(kind) = e.kind else {
                ids.push(self.enter_package(&e.name));
                continue;
            };
            let (owner, name) = self.jdk_renamed(owner, e.name);
            ids.push(self.define(Symbol {
                name,
                kind,
                owner: Some(owner),
                flags: e.flags,
                target: e.target,
                is_extern: e.is_extern,
                derived: e.derived,
                decls: Vec::new(),
                tree: Some((file, e.tree)),
            }));
        }
    }

    /// The symbol the definition at `id` of loaded file `file` defines, if it is entered.
    pub fn defined_at(&self, file: FileId, id: NodeId) -> Option<SymbolId> {
        self.by_tree.get(&(file, id)).copied()
    }

    /// How many loaded files have had their trees decoded.
    pub fn decoded_files(&self) -> usize {
        self.files.iter().filter(|f| f.is_decoded()).count()
    }

    /// Whether an annotation of the class with full name `full` annotates `sym` defined outside
    /// TASTy, or method `sym` in its TASTy by the class's simple name.
    pub fn is_annotated(&self, sym: SymbolId, full: &str) -> bool {
        let Some((file, id)) = self.symbol(sym).tree else {
            return self.annotations(sym).iter().any(|&c| self.full_name(c) == full);
        };
        let name = full.rsplit('.').next().unwrap_or(full);
        let (ast, names) = self.file(file);
        let mods = match view(ast, id) {
            View::DefDef { parts, .. } => parts.mods,
            View::ValDef { mods, .. } | View::Param { mods, .. } | View::TypeDef { mods, .. } => mods,
            _ => return false,
        };
        annotations(ast, mods).any(|(ty, _)| names_ref(ast, names, ty, name))
    }

    /// Records one more annotation of class `class` on `sym`, a symbol defined outside TASTy, with
    /// `arg` if a string literal is its only argument.
    pub fn annotate_string(&mut self, sym: SymbolId, class: SymbolId, arg: Option<String>) {
        self.annotation_strings.entry((sym, class)).or_default().push(arg);
    }

    /// The string literal arguments recorded for the annotations of class `class` on `sym`, one
    /// per annotation, in order.
    pub fn annotation_strings(&self, sym: SymbolId, class: SymbolId) -> &[Option<String>] {
        self.annotation_strings.get(&(sym, class)).map_or(&[], Vec::as_slice)
    }

    /// The first `Int` argument of the annotation of the class with full name `full` on method
    /// `sym` in its TASTy, by the class's simple name.
    pub fn annotation_int(&self, sym: SymbolId, full: &str) -> Option<i32> {
        let (file, id) = self.symbol(sym).tree?;
        let name = full.rsplit('.').next().unwrap_or(full);
        let (ast, names) = self.file(file);
        let View::DefDef { parts, .. } = view(ast, id) else { return None };
        let (_, call) = annotations(ast, parts.mods).find(|&(ty, _)| names_ref(ast, names, ty, name))?;
        first_int(ast, call)
    }

    /// The first string argument of the annotation of the class with full name `full` on method
    /// `sym` in its TASTy, by the class's simple name.
    pub fn annotation_string(&self, sym: SymbolId, full: &str) -> Option<String> {
        let (file, id) = self.symbol(sym).tree?;
        let name = full.rsplit('.').next().unwrap_or(full);
        let (ast, names) = self.file(file);
        let View::DefDef { parts, .. } = view(ast, id) else { return None };
        let (_, call) = annotations(ast, parts.mods).find(|&(ty, _)| names_ref(ast, names, ty, name))?;
        first_string(ast, names, call)
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
            Some((pkg, jdk)) => (self.enter_package(pkg), jdk),
            None => (owner, name),
        }
    }
}
