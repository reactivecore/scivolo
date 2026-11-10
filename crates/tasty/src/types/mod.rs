//! Types, hash-consed in an arena: references by name or to symbols, applied types, bounds,
//! method and lambda types.

mod decode;
mod flexible;
mod map;
mod name_table;
mod narrowing;
mod parts;
mod refined_aliases;
mod refinements;
mod show;
mod show_source;
mod show_user;
mod type_lists;
mod widen;

use crate::{FileId, SymbolId};
use rustc_hash::FxHashMap;

pub use decode::{Decoder, param_tree_clauses};
pub use name_table::NameId;
use name_table::NameTable;
pub use parts::Binding;
pub use show_source::{Highlight, escape};
use type_lists::ListTable;
pub use type_lists::{ParamList, TypeList};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TypeId(pub u32);

/// A type variable of the typer's constraint.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct VarId(pub u32);

/// The tree of a binding type (method, polymorphic, lambda or recursive type), which its
/// parameter references name.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Binder(pub FileId, pub u32);

/// One node of a type, a small value whose parts are ids into [`Types`]; matched by value
/// (`match *u.ty(t)`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Type {
    /// The prefix of a reference to a local symbol.
    NoPrefix,
    /// `prefix.name` as a type, resolved when used.
    TypeRef(TypeId, NameId),
    /// `prefix.name` as a path.
    TermRef(TypeId, NameId),
    /// A symbol of the universe, with its prefix if the reference has one.
    Sym(Option<TypeId>, SymbolId),
    /// A symbol defined in a tree the universe does not enter (a lambda's type parameter).
    Local(Binder, NameId),
    Package(SymbolId),
    /// A package the universe does not have, by qualified name (`java.lang` without javalib).
    PackageName(NameId),
    This(TypeId),
    /// `C.super[P]`: the `this` type of `C` and the parent type `P`, or all parents intersected.
    Super(TypeId, TypeId),
    Applied(TypeId, TypeList),
    Alias(TypeId),
    Bounds(TypeId, TypeId),
    And(TypeId, TypeId),
    /// A union, soft if the typer formed it as a least upper bound (widened when inferred), hard
    /// if written or read from TASTy.
    Or(TypeId, TypeId, bool),
    /// A type and the class type of its annotation.
    Annotated(TypeId, TypeId),
    /// A Java member's type `T?` (dotty's `FlexibleType`): a subtype of `T` and a supertype of
    /// `T | Null`, holding `T`.
    Flexible(TypeId),
    ByName(TypeId),
    Refined(TypeId, NameId, TypeId),
    Rec(Binder, TypeId),
    RecThis(Binder),
    /// Parameters named and typed; `implicit` for a `using` or `implicit` clause, `contextual`
    /// for a `using` one.
    Method {
        binder: Binder,
        params: ParamList,
        result: TypeId,
        implicit: bool,
        contextual: bool,
    },
    /// Type parameters named, with their bounds.
    Poly {
        binder: Binder,
        params: ParamList,
        result: TypeId,
    },
    Lambda {
        binder: Binder,
        params: ParamList,
        result: TypeId,
    },
    Param(Binder, u32),
    /// A match type: its upper bound, scrutinee and cases; a case is a `MatchCase`, or a
    /// `Lambda` over its captures whose result is one.
    Match {
        bound: TypeId,
        scrutinee: TypeId,
        cases: TypeList,
    },
    /// A match type's case: its pattern and body (dotty's `MatchCase[P, B]`).
    MatchCase(TypeId, TypeId),
    /// A constant's tag and bits; strings are `Str`, and a `CLASSconst`'s bits are the id of its
    /// class type.
    Constant(u8, i64),
    /// A string constant.
    Str(NameId),
    /// A type not represented yet, by its tag.
    Other(u8),
    /// A type variable, instantiated by the constraint the comparison runs in.
    Var(VarId),
    /// An unknown value of the given type (dotty's `SkolemType`), the binder telling it apart
    /// from the others; mapping its type makes a new one.
    Skolem(Binder, TypeId),
    /// A wildcard, with its bounds if it has any.
    Wildcard(Option<TypeId>),
    /// The type of an erroneous tree: a subtype and a supertype of every type.
    Error,
}

const _: () = assert!(size_of::<Type>() <= 24);

/// Holds the hash-consed types of a universe with their names and lists; an interned type
/// never changes.
#[derive(Default)]
pub struct Types {
    all: Vec<Type>,
    ids: FxHashMap<Type, TypeId>,
    names: NameTable,
    lists: ListTable<TypeId>,
    params: ListTable<(NameId, TypeId)>,
}

impl Types {
    /// Returns the id of type `t`, adding it when it is new: equal types share one id.
    pub fn intern(&mut self, t: Type) -> TypeId {
        if let Some(&id) = self.ids.get(&t) {
            return id;
        }
        let id = TypeId(self.all.len() as u32);
        self.all.push(t);
        self.ids.insert(t, id);
        id
    }

    /// The type with id `id`.
    pub fn get(&self, id: TypeId) -> &Type {
        &self.all[id.0 as usize]
    }

    pub fn intern_name(&mut self, text: &str) -> NameId {
        self.names.intern(text)
    }

    pub fn name(&self, id: NameId) -> &str {
        self.names.get(id)
    }
}
