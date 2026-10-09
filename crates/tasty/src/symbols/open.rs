//! Opening a universe over the TASTy of jars or of a library image: its packages known, no file
//! read yet.

use super::{Builtins, Error, Flags, Kind, Symbol, Universe, builtin, jdk};
use crate::library::Library;
use crate::types::Types;
use libimage::Image;
use rustc_hash::FxHashMap;
use std::path::PathBuf;

impl Universe {
    /// The universe of the TASTy files in `jars`.
    pub fn open(jars: &[PathBuf]) -> Result<Self, Error> {
        let library = Library::open(jars).map_err(|e| Error::Io(format!("{jars:?}"), e))?;
        Ok(Self::of(library))
    }

    /// The universe of the TASTy files in library image `image`.
    pub fn open_image(image: &'static Image) -> Self {
        Self::of(Library::Image(image))
    }

    fn of(library: Library) -> Self {
        let mut u = Self {
            library,
            symbols: Vec::new(),
            packages: FxHashMap::default(),
            pending: FxHashMap::default(),
            files: Vec::new(),
            by_tree: FxHashMap::default(),
            types: Types::default(),
            infos: FxHashMap::default(),
            parents: FxHashMap::default(),
            linearizations: FxHashMap::default(),
            deferred: Default::default(),
            with_inits: Default::default(),
            resolving: 0,
            builtins: Builtins::PENDING,
            binders: 0,
            unions: Default::default(),
            scala2_erasure: false,
        };
        let root = u.add(Symbol {
            name: String::new(),
            kind: Kind::Package,
            owner: None,
            flags: Flags::default(),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        u.packages.insert(String::new(), root);
        let mut paths: Vec<String> = u.library.paths().map(str::to_owned).collect();
        paths.sort();
        for path in paths {
            let (dir, file) = path.rsplit_once('/').unwrap_or(("", path.as_str()));
            let (dir, stem) = (dir.replace('/', "."), file.strip_suffix(".tasty").unwrap_or(file));
            let (dir, stem) = match jdk::jdk_name(&dir, stem) {
                Some((pkg, name)) => (pkg.to_owned(), name),
                None => (dir, stem.to_owned()),
            };
            let pkg = u.package_entered(&dir);
            u.pending.entry(pkg).or_default().push((stem, path.clone()));
        }
        u.builtins = builtin::enter(&mut u);
        u
    }
}
