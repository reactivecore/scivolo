//! Types, hash-consed in an arena: references by name or to symbols, applied types, bounds,
//! method and lambda types.

mod decode;
mod map;
mod show;
mod show_user;
mod widen;

use crate::{FileId, SymbolId};
use rustc_hash::FxHashMap;

pub use decode::{Decoder, param_tree_clauses};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TypeId(pub u32);

/// A type variable of the typer's constraint.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct VarId(pub u32);

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
    /// `C.super[P]`: the `this` type of `C` and the parent type `P`, or all parents intersected.
    Super(TypeId, TypeId),
    Applied(TypeId, Vec<TypeId>),
    Alias(TypeId),
    Bounds(TypeId, TypeId),
    And(TypeId, TypeId),
    Or(TypeId, TypeId),
    /// A type and the class type of its annotation.
    Annotated(TypeId, TypeId),
    ByName(TypeId),
    Refined(TypeId, String, TypeId),
    Rec(Binder, TypeId),
    RecThis(Binder),
    /// Parameters named and typed; `implicit` for a `using` or `implicit` clause, `contextual`
    /// for a `using` one.
    Method {
        binder: Binder,
        params: Vec<(String, TypeId)>,
        result: TypeId,
        implicit: bool,
        contextual: bool,
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
    /// A constant's tag and bits; strings are `Str`, and a `CLASSconst`'s bits are the id of its
    /// class type.
    Constant(u8, i64),
    /// A string constant.
    Str(String),
    /// A type not represented yet, by its tag.
    Other(u8),
    /// A type variable, instantiated by the constraint the comparison runs in.
    Var(VarId),
    /// An unknown value of the given type (dotty's `SkolemType`), the binder telling it apart
    /// from the others; mapping its type keeps the binder.
    Skolem(Binder, TypeId),
    /// A wildcard, with its bounds if it has any.
    Wildcard(Option<TypeId>),
    /// The type of an erroneous tree: a subtype and a supertype of every type.
    Error,
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
