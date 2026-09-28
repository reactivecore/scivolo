//! Interned simple names.

use rustc_hash::FxHashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Name(u32);

#[derive(Default)]
pub struct Names {
    texts: Vec<Box<str>>,
    ids: FxHashMap<Box<str>, u32>,
}

impl Names {
    pub fn intern(&mut self, text: &str) -> Name {
        if let Some(&id) = self.ids.get(text) {
            return Name(id);
        }
        let id = self.texts.len() as u32;
        self.texts.push(text.into());
        self.ids.insert(text.into(), id);
        Name(id)
    }

    pub fn text(&self, name: Name) -> &str {
        &self.texts[name.0 as usize]
    }
}
