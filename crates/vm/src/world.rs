//! Every definition loaded so far, found by global name; files load on first use.

use crate::classpath::Classpath;
use nir::{Attr, Caches, Defn, Global, GlobalId, Pool, Reader, StrId, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::hash_map::Entry;
use std::rc::Rc;

pub struct World {
    pub pool: Pool,
    classpath: Classpath,
    loaded: FxHashSet<StrId>,
    files: Vec<File>,
    /// Where each indexed definition is: its file and its offset there.
    places: FxHashMap<GlobalId, (u32, u32)>,
    defns: FxHashMap<GlobalId, Rc<Defn>>,
    members: FxHashMap<GlobalId, Vec<GlobalId>>,
}

/// A loaded NIR file, whose definitions decode on first use.
struct File {
    data: Vec<u8>,
    caches: Caches,
}

#[derive(Debug)]
pub enum LoadError {
    Missing(String),
    Corrupt(String, nir::Error),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Missing(name) => write!(f, "no definition of {name} on the classpath"),
            LoadError::Corrupt(name, e) => write!(f, "{name}: {e}"),
        }
    }
}

impl World {
    pub fn new(classpath: Classpath) -> Self {
        Self {
            pool: Pool::default(),
            classpath,
            loaded: FxHashSet::default(),
            files: Vec::new(),
            places: FxHashMap::default(),
            defns: FxHashMap::default(),
            members: FxHashMap::default(),
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

    /// The classpath's resource files, see `Classpath::resources`.
    pub fn resources(&mut self) -> Vec<(String, Vec<u8>)> {
        self.classpath.resources()
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
        self.load(self.owner(g))?;
        let Some(&(file, offset)) = self.places.get(&g) else {
            return Err(LoadError::Missing(self.name(g)));
        };
        let file = &mut self.files[file as usize];
        Reader::resume(&file.data, &mut file.caches, &mut self.pool)
            .and_then(|mut r| f(&mut r, offset))
            .map_err(|e| LoadError::Corrupt(self.pool.top_name(g).to_owned(), e))
    }

    /// The members defined in the file of top-level `top`, in file order.
    pub fn members(&mut self, top: GlobalId) -> Result<&[GlobalId], LoadError> {
        self.load(top)?;
        Ok(self.members.get(&top).map_or(&[], Vec::as_slice))
    }

    fn load(&mut self, top: GlobalId) -> Result<(), LoadError> {
        let Global::Top(name) = *self.pool.global(top) else {
            return Err(LoadError::Missing(self.name(top)));
        };
        if !self.loaded.insert(name) {
            return Ok(());
        }
        let text = self.pool.str(name).to_owned();
        let data = self.classpath.find(&text).ok_or_else(|| LoadError::Missing(text.clone()))?;
        let mut caches = Caches::default();
        let index = Reader::new(&data, &mut caches, &mut self.pool)
            .and_then(|mut r| r.index())
            .map_err(|e| LoadError::Corrupt(text, e))?;
        let file = self.files.len() as u32;
        for (g, offset) in index {
            let Entry::Vacant(place) = self.places.entry(g) else { continue };
            place.insert((file, offset));
            if let Global::Member(owner, _) = *self.pool.global(g) {
                self.members.entry(owner).or_default().push(g);
            }
        }
        self.files.push(File { data, caches });
        Ok(())
    }
}
