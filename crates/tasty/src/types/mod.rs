//! Types, hash-consed in an arena: references by name or to symbols, applied types, bounds,
//! method and lambda types.

mod decode;
mod map;
mod show;

use crate::{FileId, SymbolId};
use rustc_hash::FxHashMap;

pub use decode::Decoder;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TypeId(pub u32);

/// The tree of a binding type (method, polymorphic, lambda or recursive type), which its
/// parameter references name.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Binder(pub FileId, pub u32);

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Type {
    /// The prefix of a reference to a local symbol.
    NoPrefix,
    /// `prefix.name` as a type, resolved when used.
    TypeRef(TypeId, String),
    /// `prefix.name` as a path.
    TermRef(TypeId, String),
    /// A symbol of the universe, with its prefix if the reference has one.
    Sym(Option<TypeId>, SymbolId),
    /// A symbol defined in a tree the universe does not enter (a lambda's type parameter).
    Local(Binder, String),
    Package(SymbolId),
    /// A package the universe does not have, by qualified name (`java.lang` without javalib).
    PackageName(String),
    This(TypeId),
    Applied(TypeId, Vec<TypeId>),
    Alias(TypeId),
    Bounds(TypeId, TypeId),
    And(TypeId, TypeId),
    Or(TypeId, TypeId),
    Annotated(TypeId),
    ByName(TypeId),
    Refined(TypeId, String, TypeId),
    Rec(Binder, TypeId),
    RecThis(Binder),
    /// Parameters named and typed; `implicit` for a `using` or `implicit` clause.
    Method {
        binder: Binder,
        params: Vec<(String, TypeId)>,
        result: TypeId,
        implicit: bool,
    },
    /// Type parameters named, with their bounds.
    Poly {
        binder: Binder,
        params: Vec<(String, TypeId)>,
        result: TypeId,
    },
    Lambda {
        binder: Binder,
        params: Vec<(String, TypeId)>,
        result: TypeId,
    },
    Param(Binder, u32),
    /// A match type, by its upper bound.
    Match(TypeId),
    /// A constant's tag and bits.
    Constant(u8, i64),
    /// A type not represented yet, by its tag.
    Other(u8),
}

#[derive(Default)]
pub struct Types {
    all: Vec<Type>,
    ids: FxHashMap<Type, TypeId>,
}

impl Types {
    pub fn intern(&mut self, t: Type) -> TypeId {
        if let Some(&id) = self.ids.get(&t) {
            return id;
        }
        let id = TypeId(self.all.len() as u32);
        self.all.push(t.clone());
        self.ids.insert(t, id);
        id
    }

    pub fn get(&self, id: TypeId) -> &Type {
        &self.all[id.0 as usize]
    }
}
