//! Every definition loaded so far, found by global name; files load on first use.

use crate::classpath::Classpath;
use crate::member_table::Table;
use nir::{Attr, Caches, Defn, Global, GlobalId, Pool, Reader, StrId, TypeId};
use rustc_hash::FxHashMap;
use std::collections::hash_map::Entry;
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
}

/// A loaded NIR file, whose definitions decode on first use; with a member table, its index
/// is not decoded.
struct File {
    data: std::borrow::Cow<'static, [u8]>,
    caches: Caches,
    table: Option<Table>,
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
            loaded: FxHashMap::default(),
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

    /// The file and offset of `g`'s definition, loading its owner's file first.
    fn place(&mut self, g: GlobalId) -> Result<Option<(u32, u32)>, LoadError> {
        let file = self.load(self.owner(g))?;
        if let Some(&place) = self.places.get(&g) {
            return Ok(Some(place));
        }
        let Some(table) = file.and_then(|f| self.files[f as usize].table) else { return Ok(None) };
        let offset = match *self.pool.global(g) {
            Global::Member(_, sig) => table.find(self.pool.str(sig)),
            _ => table.top(),
        };
        Ok(file.zip(offset).inspect(|&place| {
            self.places.insert(g, place);
        }))
    }

    /// The member table of top-level `top`'s file, if it has one.
    fn table(&mut self, top: GlobalId) -> Result<Option<Table>, LoadError> {
        Ok(self.load(top)?.and_then(|f| self.files[f as usize].table))
    }

    /// The members defined in the file of top-level `top`, in file order.
    pub fn members(&mut self, top: GlobalId) -> Result<&[GlobalId], LoadError> {
        if let Some(table) = self.table(top)?
            && !self.members.contains_key(&top)
        {
            let members = table.members().map(|(sig, _, _)| self.member(top, sig)).collect();
            self.members.insert(top, members);
        }
        Ok(self.members.get(&top).map_or(&[], Vec::as_slice))
    }

    /// The members of top-level `top` that may be fields, in file order: the fields its member
    /// table lists, else all members.
    pub fn fields(&mut self, top: GlobalId) -> Result<Vec<GlobalId>, LoadError> {
        match self.table(top)? {
            Some(table) => {
                Ok(table.members().filter(|m| m.2).map(|(sig, _, _)| self.member(top, sig)).collect())
            }
            None => Ok(self.members(top)?.to_vec()),
        }
    }

    /// The file of top-level `top`, loaded on first use; `None` if the classpath has none or it
    /// cannot be read.
    fn load(&mut self, top: GlobalId) -> Result<Option<u32>, LoadError> {
        let Global::Top(name) = *self.pool.global(top) else {
            return Err(LoadError::Missing(self.name(top)));
        };
        if let Some(&file) = self.loaded.get(&name) {
            return Ok(file);
        }
        self.loaded.insert(name, None);
        let text = self.pool.str(name).to_owned();
        let Some((data, table)) = self.classpath.find(&text) else { return Ok(None) };
        let file = self.files.len() as u32;
        let mut caches = Caches::default();
        let mut reader = Reader::new(&data, &mut caches, &mut self.pool)
            .map_err(|e| LoadError::Corrupt(text.clone(), e))?;
        if table.is_none() {
            for (g, offset) in reader.index().map_err(|e| LoadError::Corrupt(text, e))? {
                let Entry::Vacant(place) = self.places.entry(g) else { continue };
                place.insert((file, offset));
                if let Global::Member(owner, _) = *self.pool.global(g) {
                    self.members.entry(owner).or_default().push(g);
                }
            }
        }
        self.files.push(File { data, caches, table });
        self.loaded.insert(name, Some(file));
        Ok(Some(file))
    }
}
