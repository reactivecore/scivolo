//! The symbols of a library: packages known up front, each top-level file's classes and members
//! entered when a name it defines is first looked up.

mod enter;
mod info;
mod lookup;
mod resolve;
mod symbol;

use crate::library::Library;
use crate::types::{TypeId, Types};
use crate::{Ast, NodeId, Tasty};
use rustc_hash::FxHashMap;
use std::io;
use std::path::PathBuf;

pub use lookup::encode;
pub use symbol::{FileId, Flags, Kind, Symbol, SymbolId};

/// A decoded top-level file.
struct Loaded {
    bytes: Box<[u8]>,
    ast: Ast,
}

pub struct Universe {
    library: Library,
    symbols: Vec<Symbol>,
    packages: FxHashMap<String, SymbolId>,
    /// Per package, the files not loaded yet: the file name without `.tasty`, and its path.
    pending: FxHashMap<SymbolId, Vec<(String, String)>>,
    files: Vec<Loaded>,
    /// The symbol each definition tree defines.
    by_tree: FxHashMap<(FileId, NodeId), SymbolId>,
    pub types: Types,
    /// The declared type of each symbol whose type was asked for.
    infos: FxHashMap<SymbolId, TypeId>,
}

#[derive(Debug)]
pub enum Error {
    Io(String, io::Error),
    Corrupt(String, crate::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(path, e) => write!(f, "{path}: {e}"),
            Error::Corrupt(path, e) => write!(f, "{path}: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl Universe {
    /// The packages of the TASTy files in `jars`, no file read yet.
    pub fn open(jars: &[PathBuf]) -> Result<Self, Error> {
        let library = Library::open(jars).map_err(|e| Error::Io(format!("{jars:?}"), e))?;
        let mut u = Self {
            library,
            symbols: Vec::new(),
            packages: FxHashMap::default(),
            pending: FxHashMap::default(),
            files: Vec::new(),
            by_tree: FxHashMap::default(),
            types: Types::default(),
            infos: FxHashMap::default(),
        };
        let root = u.add(Symbol {
            name: String::new(),
            kind: Kind::Package,
            owner: None,
            flags: Flags::default(),
            decls: Vec::new(),
            tree: None,
        });
        u.packages.insert(String::new(), root);
        let mut paths: Vec<String> = u.library.paths().map(str::to_owned).collect();
        paths.sort();
        for path in paths {
            let (dir, file) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
            let pkg = u.package_entered(&dir.replace('/', "."));
            let stem = file.strip_suffix(".tasty").unwrap_or(file).to_owned();
            u.pending.entry(pkg).or_default().push((stem, path.clone()));
        }
        Ok(u)
    }

    pub fn root(&self) -> SymbolId {
        SymbolId(0)
    }

    pub fn symbol(&self, id: SymbolId) -> &Symbol {
        &self.symbols[id.0 as usize]
    }

    /// The package of qualified name `name`, e.g. `scala.collection`, if the library has it.
    pub fn package(&self, name: &str) -> Option<SymbolId> {
        self.packages.get(name).copied()
    }

    /// The qualified name of `id`, e.g. `scala.collection.immutable.List`.
    pub fn full_name(&self, id: SymbolId) -> String {
        let s = self.symbol(id);
        match s.owner {
            Some(o) if o != self.root() => format!("{}.{}", self.full_name(o), s.name),
            _ => s.name.clone(),
        }
    }

    /// How many files are loaded.
    pub fn loaded_files(&self) -> usize {
        self.files.len()
    }

    /// The decoded header, names and sections of loaded file `file`, and its trees.
    pub fn file(&self, file: FileId) -> (Tasty<'_>, &Ast) {
        let f = &self.files[file.0 as usize];
        (Tasty::parse(&f.bytes).expect("parsed when loaded"), &f.ast)
    }

    /// The package of qualified name `name`, entered with its enclosing packages if new;
    /// `<empty>`, the package of files at the root of a jar, is the root.
    fn package_entered(&mut self, name: &str) -> SymbolId {
        let name = if name == "<empty>" { "" } else { name };
        if let Some(&p) = self.packages.get(name) {
            return p;
        }
        let (outer, simple) = name.rsplit_once('.').unwrap_or(("", name));
        let owner = self.package_entered(outer);
        let id = self.add(Symbol {
            name: simple.to_owned(),
            kind: Kind::Package,
            owner: Some(owner),
            flags: Flags::default(),
            decls: Vec::new(),
            tree: None,
        });
        self.packages.insert(name.to_owned(), id);
        id
    }

    fn add(&mut self, s: Symbol) -> SymbolId {
        let id = SymbolId(self.symbols.len() as u32);
        if let Some(o) = s.owner {
            self.symbols[o.0 as usize].decls.push(id);
        }
        if let Some(tree) = s.tree {
            self.by_tree.insert(tree, id);
        }
        self.symbols.push(s);
        id
    }
}
