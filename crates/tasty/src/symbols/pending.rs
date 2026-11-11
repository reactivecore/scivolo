//! Loading the pending files of a package: those defining a name, the package objects' or all.

use super::companions::module_name;
use super::lookup::encode;
use super::predicates::is_package_object_name;
use super::{Error, SymbolId, Universe};
use crate::types::NameId;
use rustc_hash::FxHashMap;
use std::borrow::Cow;

/// The stems of a package's pending files, built on the first lookup by name.
#[derive(Default)]
pub struct PendingStems {
    /// How many pending files have each stem.
    counts: FxHashMap<NameId, u32>,
    /// How many pending files hold package objects.
    package_objects: u32,
}

impl PendingStems {
    fn add(&mut self, (id, stem): (NameId, &str), n: i32) {
        let count = self.counts.entry(id).or_default();
        *count = count.wrapping_add_signed(n);
        if *count == 0 {
            self.counts.remove(&id);
        }
        if is_package_object_file(stem) {
            self.package_objects = self.package_objects.wrapping_add_signed(n);
        }
    }
}

fn is_package_object_file(stem: &str) -> bool {
    stem == "package" || stem.ends_with("$package")
}

/// `encode(name)`, borrowing `name` where it is spelled as is.
fn encoded(name: &str) -> Cow<'_, str> {
    let ascii = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$';
    let plain = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    match name.bytes().all(ascii) || name.chars().all(plain) {
        true => Cow::Borrowed(name),
        false => Cow::Owned(encode(name)),
    }
}

impl Universe {
    /// Loads the pending files of package `pkg` that define `name`: its encoded name, or an
    /// object's class `C$` the file of `C`.
    pub fn load_defining(&mut self, pkg: SymbolId, name: NameId) -> Result<(), Error> {
        if !self.pending.contains_key(&pkg) {
            return Ok(());
        }
        let stem = match encoded(self.name(name)) {
            Cow::Borrowed(_) => name,
            Cow::Owned(e) => self.intern_name(&e),
        };
        let module = module_name(self.name(stem)).map(str::to_owned).map(|m| self.intern_name(&m));
        let stems = self.pending_stems(pkg);
        if !stems.counts.contains_key(&stem) && module.is_none_or(|m| !stems.counts.contains_key(&m)) {
            return Ok(());
        }
        let encoded = self.name(stem).to_owned();
        let module = module.map(|m| self.name(m).to_owned());
        self.load_pending(pkg, |file| file == encoded || Some(file) == module.as_deref())
    }

    /// The classes of the package objects of package `pkg`, loading only their files.
    pub fn package_objects(&mut self, pkg: SymbolId) -> Result<Vec<SymbolId>, Error> {
        if self.pending.contains_key(&pkg) && self.pending_stems(pkg).package_objects > 0 {
            self.load_pending(pkg, is_package_object_file)?;
        }
        let key = (self.generation, self.symbol(pkg).decls.len());
        if let Some((k, objects)) = self.package_objects.get(&pkg)
            && *k == key
        {
            return Ok(objects.clone());
        }
        let decls = &self.symbol(pkg).decls;
        let objects: Vec<SymbolId> = decls
            .iter()
            .copied()
            .filter(|&d| self.is_class(d) && is_package_object_name(&self.symbol(d).name))
            .collect();
        self.package_objects.insert(pkg, (key, objects.clone()));
        Ok(objects)
    }

    /// All declarations of `owner`, loading every file of a package.
    pub fn decls(&mut self, owner: SymbolId) -> Result<Vec<SymbolId>, Error> {
        self.load_pending(owner, |_| true)?;
        Ok(self.symbol(owner).decls.clone())
    }

    fn pending_stems(&mut self, pkg: SymbolId) -> &PendingStems {
        let Universe { pending, pending_stems, types, .. } = self;
        pending_stems.entry(pkg).or_insert_with(|| {
            let mut stems = PendingStems::default();
            for (stem, _) in pending.get(&pkg).into_iter().flatten() {
                stems.add((types.intern_name(stem), stem), 1);
            }
            stems
        })
    }

    /// Loads the pending files of `pkg` whose names `wanted` accepts, one at a time; a file that
    /// fails stays pending, so each later lookup reports it again.
    fn load_pending(&mut self, pkg: SymbolId, wanted: impl Fn(&str) -> bool) -> Result<(), Error> {
        loop {
            let Some(files) = self.pending.get(&pkg) else { return Ok(()) };
            let Some(i) = files.iter().position(|(stem, _)| wanted(stem)) else { return Ok(()) };
            let path = files[i].1.clone();
            self.load(&path)?;
            let Some(files) = self.pending.get_mut(&pkg) else { continue };
            let (stem, _) = files.remove(i);
            if let Some(stems) = self.pending_stems.get_mut(&pkg) {
                stems.add((self.types.intern_name(&stem), &stem), -1);
            }
        }
    }
}
