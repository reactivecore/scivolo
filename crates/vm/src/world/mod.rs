//! Every definition loaded so far, found by global name; files load on first use.

mod files;
mod overlay;

use crate::classpath::Classpath;
use files::File;
use nir::{Attr, Defn, Global, GlobalId, Pool, Reader, StrId, TypeId};
use rustc_hash::FxHashMap;
use std::rc::Rc;

pub struct World {
    pub pool: Pool,
    classpath: Classpath,
    /// The file of each top-level name looked up so far, `None` if it has none.
    loaded: FxHashMap<StrId, Option<u32>>,
    files: Vec<File>,
    /// Where each definition found so far is: its file and its offset there.
    places: FxHashMap<GlobalId, (u32, u32)>,
    defns: FxHashMap<GlobalId, Rc<Defn>>,
    members: FxHashMap<GlobalId, Vec<GlobalId>>,
    /// Top-level names whose overlay does not match the library, with the mismatch.
    broken: FxHashMap<StrId, String>,
}

#[derive(Debug)]
pub enum LoadError {
    Missing(String),
    Corrupt(String, nir::Error),
    /// An overlay member the library does not match.
    Overlay(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Missing(name) => write!(f, "no definition of {name} on the classpath"),
            LoadError::Corrupt(name, e) => write!(f, "{name}: {e}"),
            LoadError::Overlay(what) => write!(f, "overlay: {what}"),
        }
    }
}

impl World {
    pub fn new(classpath: Classpath) -> Self {
        Self {
            pool: Pool::default(),
            classpath,
            loaded: FxHashMap::default(),
            files: Vec::new(),
            places: FxHashMap::default(),
            defns: FxHashMap::default(),
            members: FxHashMap::default(),
            broken: FxHashMap::default(),
        }
    }

    pub fn top(&mut self, name: &str) -> GlobalId {
        let s = StrId(self.pool.strings.intern_str(name));
        GlobalId(self.pool.globals.intern(Global::Top(s)))
    }

    pub fn member(&mut self, owner: GlobalId, sig: &str) -> GlobalId {
        let s = StrId(self.pool.strings.intern_str(sig));
        GlobalId(self.pool.globals.intern(Global::Member(owner, s)))
    }

    /// The top-level names on the classpath that end in `suffix`.
    pub fn tops_ending(&mut self, suffix: &str) -> Vec<String> {
        self.classpath.tops_ending(suffix)
    }

    /// The classpath's resource files as root index and path, see `Classpath::resources`.
    pub fn resources(&mut self) -> Vec<(usize, String)> {
        self.classpath.resources()
    }

    /// The contents of resource `rel` in classpath root `root`.
    pub fn resource(&mut self, root: usize, rel: &str) -> Option<Vec<u8>> {
        self.classpath.read_at(root, rel)
    }

    /// The contents of file `rel` in each classpath directory and jar that has one.
    pub fn classpath_files(&mut self, rel: &str) -> Vec<Vec<u8>> {
        self.classpath.all(rel)
    }

    pub fn owner(&self, g: GlobalId) -> GlobalId {
        match self.pool.global(g) {
            Global::Member(owner, _) => *owner,
            _ => g,
        }
    }

    pub fn name(&self, g: GlobalId) -> String {
        nir::show::global(&self.pool, g, true)
    }

    /// The definition of `g`, loading its file and decoding it on first use.
    pub fn defn(&mut self, g: GlobalId) -> Result<Rc<Defn>, LoadError> {
        if let Some(d) = self.defns.get(&g) {
            return Ok(d.clone());
        }
        let defn = Rc::new(self.read(g, |r, offset| {
            r.debug_info = false;
            r.defn_at(offset)
        })?);
        self.defns.insert(g, defn.clone());
        Ok(defn)
    }

    /// Adds `defn`, which no file defines, under its name.
    pub fn define(&mut self, defn: Defn) {
        self.defns.insert(defn.name, Rc::new(defn));
    }

    /// The type and attributes of member `g` if it is a `var`, without decoding it whole.
    pub fn var(&mut self, g: GlobalId) -> Result<Option<(TypeId, Vec<Attr>)>, LoadError> {
        self.read(g, |r, offset| r.var_at(offset))
    }

    /// What `f` reads with a reader of `g`'s file, given `g`'s offset in it.
    fn read<T>(
        &mut self,
        g: GlobalId,
        f: impl FnOnce(&mut Reader, u32) -> nir::Result<T>,
    ) -> Result<T, LoadError> {
        let Some((file, offset)) = self.place(g)? else {
            return Err(LoadError::Missing(self.pool.top_name(g).to_owned()));
        };
        let file = &mut self.files[file as usize];
        Reader::resume(&file.data, &mut file.caches, &mut self.pool)
            .and_then(|mut r| f(&mut r, offset))
            .map_err(|e| LoadError::Corrupt(self.pool.top_name(g).to_owned(), e))
    }
}
