//! Opening a universe over the TASTy of jars or of a library image: its packages known, no file
//! read yet.

use super::{Builtins, Error, Flags, Kind, Symbol, Universe, builtin};
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

    /// The universe of no library: its builtins only.
    pub fn empty() -> Self {
        Self::of(Library::Jars { jars: Vec::new(), files: FxHashMap::default() })
    }

    fn of(library: Library) -> Self {
        let mut u = Self {
            library,
            symbols: Vec::new(),
            packages: FxHashMap::default(),
            pending: FxHashMap::default(),
            files: Vec::new(),
            by_tree: FxHashMap::default(),
            by_name: FxHashMap::default(),
            resolved: FxHashMap::default(),
            generation: 0,
            erased_methods: FxHashMap::default(),
            types: Types::default(),
            infos: FxHashMap::default(),
            parents: FxHashMap::default(),
            linearizations: FxHashMap::default(),
            deferred: Default::default(),
            with_inits: Default::default(),
            annotations: Default::default(),
            opaque_aliases: Default::default(),
            opaques_elided: false,
            comparison_owner: None,
            opaque_lifts: 0,
            resolving: 0,
            builtins: Builtins::PENDING,
            binders: 0,
            unions: Default::default(),
            subtype_steps: 0,
            scala2_erasure: false,
        };
        let root = u.define(Symbol {
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
        let table = u.library.package_table();
        for line in table.lines() {
            let mut fields = line.split('\t');
            let (Some(pkg), Some(stem), Some(path)) = (fields.next(), fields.next(), fields.next()) else {
                continue;
            };
            let pkg = u.package_entered(pkg);
            u.pending.entry(pkg).or_default().push((stem.to_owned(), path.to_owned()));
        }
        u.builtins = builtin::enter(&mut u);
        u
    }
}
