//! The NIR files behind the world: loading them and finding a definition's place in them.

use super::{LoadError, World};
use crate::member_table::Table;
use nir::{Caches, Global, GlobalId, Reader, StrId};
use std::borrow::Cow;
use std::collections::hash_map::Entry;

/// A loaded NIR file, whose definitions decode on first use; with a member table, its index
/// is not decoded.
pub struct File {
    pub data: Cow<'static, [u8]>,
    pub caches: Caches,
    pub table: Option<Table>,
}

impl World {
    /// The file and offset of `g`'s definition, loading its owner's file first.
    pub fn place(&mut self, g: GlobalId) -> Result<Option<(u32, u32)>, LoadError> {
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
        if let Some(mismatch) = self.broken.get(&name) {
            return Err(LoadError::Overlay(mismatch.clone()));
        }
        if let Some(&file) = self.loaded.get(&name) {
            return Ok(file);
        }
        self.loaded.insert(name, None);
        let text = self.pool.str(name).to_owned();
        let overlay = super::overlay::file(&text);
        let (found, overlaid) = match (self.classpath.find(&text), overlay) {
            (Some(found), overlay) => (found, overlay),
            (None, Some(data)) => ((Cow::Borrowed(data), None), None),
            (None, None) => return Ok(None),
        };
        let (data, table) = found;
        let file = self.index(name, text, data, table)?;
        if let Some(data) = overlaid {
            self.merge(top, data)?;
        }
        Ok(Some(file))
    }

    /// Adds NIR file `data` as the file of top-level `top`; an error if `top` was looked up or
    /// added before.
    pub fn add_file(&mut self, top: &str, data: Vec<u8>) -> Result<(), LoadError> {
        let name = StrId(self.pool.strings.intern_str(top));
        if self.loaded.contains_key(&name) || self.broken.contains_key(&name) {
            return Err(LoadError::Defined(top.to_owned()));
        }
        self.index(name, top.to_owned(), Cow::Owned(data), None).map(drop)
    }

    /// Adds `data` as the file of top-level name `name` (`text`), indexing its definitions unless
    /// it has a member table.
    fn index(
        &mut self,
        name: StrId,
        text: String,
        data: Cow<'static, [u8]>,
        table: Option<Table>,
    ) -> Result<u32, LoadError> {
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
        Ok(file)
    }
}
