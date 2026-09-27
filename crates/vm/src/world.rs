//! Every definition loaded so far, found by global name; files load on first use.

use crate::classpath::Classpath;
use nir::{Caches, Defn, Global, GlobalId, Pool, Reader, StrId};
use rustc_hash::{FxHashMap, FxHashSet};
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
        let s = StrId(self.pool.strings.intern(name.into()));
        GlobalId(self.pool.globals.intern(Global::Top(s)))
    }

    pub fn member(&mut self, owner: GlobalId, sig: &str) -> GlobalId {
        let s = StrId(self.pool.strings.intern(sig.into()));
        GlobalId(self.pool.globals.intern(Global::Member(owner, s)))
    }

    /// The top-level names on the classpath that end in `suffix`.
    pub fn tops_ending(&mut self, suffix: &str) -> Vec<String> {
        self.classpath.tops_ending(suffix)
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
        self.load(self.owner(g))?;
        let Some(&(file, offset)) = self.places.get(&g) else {
            return Err(LoadError::Missing(self.name(g)));
        };
        let f = &mut self.files[file as usize];
        let defn = Reader::resume(&f.data, &mut f.caches, &mut self.pool)
            .and_then(|mut r| {
                r.debug_info = false;
                r.defn_at(offset)
            })
            .map_err(|e| LoadError::Corrupt(self.pool.top_name(g).to_owned(), e))?;
        let defn = Rc::new(defn);
        self.defns.insert(g, defn.clone());
        Ok(defn)
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
            if let Global::Member(owner, _) = *self.pool.global(g) {
                let list = self.members.entry(owner).or_default();
                if !list.contains(&g) {
                    list.push(g);
                }
            }
            self.places.entry(g).or_insert((file, offset));
        }
        self.files.push(File { data, caches });
        Ok(())
    }
}
