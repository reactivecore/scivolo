//! The declared types of symbols, decoded from their trees on first use.

use super::{FileId, Kind, SymbolId, Universe};
use crate::tags::{OPAQUE, TEMPLATE};
use crate::types::{Decoder, Type, TypeId};

impl Universe {
    /// The declared type of `sym`: a method's type, a value's type, a type member's alias or
    /// bounds, a type parameter's bounds; `Other(TEMPLATE)` for classes and packages.
    pub fn info(&mut self, sym: SymbolId) -> TypeId {
        if let Some(&t) = self.infos.get(&sym) {
            return t;
        }
        let (kind, tree) = (self.symbol(sym).kind, self.symbol(sym).tree);
        let t = match (kind, tree) {
            (Kind::Package | Kind::Class, _) | (_, None) => self.types.intern(Type::Other(TEMPLATE)),
            (_, Some((file, id))) => self.with_decoder(file, |d| d.definition_type(id)),
        };
        self.infos.insert(sym, t);
        t
    }

    /// The lower and upper bound of opaque type `sym`, when its definition states them.
    pub fn opaque_bounds(&mut self, sym: SymbolId) -> Option<(TypeId, TypeId)> {
        let (file, id) = self.symbol(sym).tree?;
        if !self.symbol(sym).flags.has(OPAQUE) {
            return None;
        }
        self.with_decoder(file, |d| d.opaque_bounds(id))
    }

    fn with_decoder<R>(&mut self, file: FileId, f: impl FnOnce(&mut Decoder) -> R) -> R {
        let Universe { files, by_tree, packages, types, .. } = self;
        let loaded = &files[file.0 as usize];
        let mut d = Decoder::new((file, &loaded.ast, &loaded.names), by_tree, packages, types);
        f(&mut d)
    }
}
