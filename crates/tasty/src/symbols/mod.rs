//! The symbols of a library: packages known up front, each top-level file's classes and members
//! entered when a name it defines is first looked up.

mod enter;

use crate::library::Library;
use crate::{Ast, NodeId, Tasty};
use rustc_hash::FxHashMap;
use std::io;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SymbolId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Package,
    /// A class, trait or module class, with its members.
    Class,
    /// A type alias, abstract type or opaque type.
    Type,
    TypeParam,
    Val,
    Def,
}

/// Modifiers, one bit per TASTy modifier tag (`tags::PRIVATE`… `tags::INTO`).
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Flags(pub u64);

impl Flags {
    pub fn has(self, tag: u8) -> bool {
        self.0 & (1 << tag) != 0
    }
}

pub struct Symbol {
    pub name: String,
    pub kind: Kind,
    pub owner: Option<SymbolId>,
    pub flags: Flags,
    /// The symbols declared in a package or class, in file order.
    pub decls: Vec<SymbolId>,
    /// The file and tree that define it; `None` for packages.
    pub tree: Option<(u32, NodeId)>,
}

/// A decoded top-level file; its bytes stay for the life of the process.
pub struct Loaded {
    pub tasty: Tasty<'static>,
    pub ast: Ast,
}

pub struct Universe {
    library: Library,
    symbols: Vec<Symbol>,
    packages: FxHashMap<String, SymbolId>,
    /// Per package, the files not loaded yet: the file name without `.tasty`, and its path.
    pending: FxHashMap<SymbolId, Vec<(String, String)>>,
    pub files: Vec<Loaded>,
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Corrupt(String, crate::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Corrupt(path, e) => write!(f, "{path}: {e}"),
        }
    }
}

impl Universe {
    /// The packages of the TASTy files in `jars`, no file read yet.
    pub fn open(jars: &[PathBuf]) -> Result<Self, Error> {
        let library = Library::open(jars).map_err(Error::Io)?;
        let root = Symbol {
            name: String::new(),
            kind: Kind::Package,
            owner: None,
            flags: Flags::default(),
            decls: Vec::new(),
            tree: None,
        };
        let mut u = Self {
            library,
            symbols: vec![root],
            packages: FxHashMap::default(),
            pending: FxHashMap::default(),
            files: Vec::new(),
        };
        u.packages.insert(String::new(), SymbolId(0));
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

    /// The declarations of `owner` named `name`, loading the package's file that defines it.
    pub fn decl(&mut self, owner: SymbolId, name: &str) -> Result<Vec<SymbolId>, Error> {
        let found = |u: &Self| -> Vec<SymbolId> {
            u.symbol(owner).decls.iter().copied().filter(|&d| u.symbol(d).name == name).collect()
        };
        if self.symbol(owner).kind == Kind::Package && found(self).is_empty() {
            let stem = name.strip_suffix('$').unwrap_or(name);
            self.load_pending(owner, |file| file == stem)?;
            if found(self).is_empty() {
                self.load_pending(owner, |file| file == "package" || file.ends_with("$package"))?;
            }
        }
        Ok(found(self))
    }

    /// All declarations of `owner`, loading every file of a package.
    pub fn decls(&mut self, owner: SymbolId) -> Result<Vec<SymbolId>, Error> {
        self.load_pending(owner, |_| true)?;
        Ok(self.symbol(owner).decls.clone())
    }

    fn load_pending(&mut self, pkg: SymbolId, wanted: impl Fn(&str) -> bool) -> Result<(), Error> {
        let Some(files) = self.pending.get_mut(&pkg) else { return Ok(()) };
        let (load, keep): (Vec<_>, Vec<_>) =
            std::mem::take(files).into_iter().partition(|(stem, _)| wanted(stem));
        *files = keep;
        for (_, path) in load {
            self.load(&path)?;
        }
        Ok(())
    }

    /// The package of qualified name `name`, entered with its enclosing packages if new.
    fn package_entered(&mut self, name: &str) -> SymbolId {
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
        self.symbols.push(s);
        id
    }
}
