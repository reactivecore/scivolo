//! Entering the symbols a top-level file defines: from the library image's symbol table of the
//! file if it has one, else from the decoded file.

use super::entries::{Entry, entries};
use super::loaded::{Decoded, Loaded};
use super::{Error, FileId, Kind, Symbol, SymbolId, Universe, table};
use crate::NodeId;

impl Universe {
    /// Loads the file at `path` and enters its symbols, and the JDK's declarations over them.
    pub fn load(&mut self, path: &str) -> Result<(), Error> {
        let (file, entries) = self.read_entries(path)?;
        self.enter_entries(file, entries);
        self.load_jdk_declarations(path)
    }

    /// Loads the file at `path` and the entries of the symbols it defines.
    pub fn read_entries(&mut self, path: &str) -> Result<(FileId, Vec<Entry>), Error> {
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
        Ok((file, entries))
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

    /// The owner and name of a definition named `name` in `owner`: its JDK package and name if
    /// Scala Native defines it under another name.
    pub fn jdk_renamed(&mut self, owner: SymbolId, name: String) -> (SymbolId, String) {
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
