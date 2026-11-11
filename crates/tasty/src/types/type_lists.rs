//! The lists that types hold, interned: the elements of all lists one after another in one
//! buffer.

use super::{Binder, NameId, Type, TypeId, Types};
use hashbrown::HashTable;
use rustc_hash::FxBuildHasher;
use std::hash::{BuildHasher, Hash};

/// A list of types: the arguments of an `Applied` type or the cases of a `Match` type.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TypeList(u32);

/// The parameters of a `Method`, `Poly` or `Lambda` type, named and typed.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ParamList(u32);

/// Hash-conses lists of `T`: equal lists get the same id.
pub struct ListTable<T> {
    elems: Vec<T>,
    /// Where each list starts in `elems`, then where the last one ends.
    bounds: Vec<u32>,
    ids: HashTable<u32>,
}

impl<T> Default for ListTable<T> {
    fn default() -> Self {
        ListTable { elems: Vec::new(), bounds: vec![0], ids: HashTable::new() }
    }
}

fn hash<T: Hash>(list: &[T]) -> u64 {
    FxBuildHasher.hash_one(list)
}

fn list_of<'a, T>(elems: &'a [T], bounds: &[u32], id: u32) -> &'a [T] {
    &elems[bounds[id as usize] as usize..bounds[id as usize + 1] as usize]
}

impl<T: Copy + Eq + Hash> ListTable<T> {
    fn intern(&mut self, list: &[T]) -> u32 {
        let h = hash(list);
        let ListTable { elems, bounds, ids } = self;
        if let Some(&id) = ids.find(h, |&id| list_of(elems, bounds, id) == list) {
            return id;
        }
        elems.extend_from_slice(list);
        let id = u32::try_from(bounds.len() - 1).expect("fewer than 2^32 lists");
        bounds.push(u32::try_from(elems.len()).expect("fewer than 2^32 list elements"));
        ids.insert_unique(h, id, |&id| hash(list_of(elems, bounds, id)));
        id
    }

    fn get(&self, id: u32) -> &[T] {
        list_of(&self.elems, &self.bounds, id)
    }
}

impl Types {
    /// Interns type list `list`, returning the id an equal list got before if any.
    pub fn intern_types(&mut self, list: &[TypeId]) -> TypeList {
        TypeList(self.lists.intern(list))
    }

    /// The types of list `list`.
    pub fn types_of(&self, list: TypeList) -> &[TypeId] {
        self.lists.get(list.0)
    }

    /// Interns parameter list `list`, returning the id an equal list got before if any.
    pub fn intern_params(&mut self, list: &[(NameId, TypeId)]) -> ParamList {
        ParamList(self.params.intern(list))
    }

    /// The parameters of list `list`.
    pub fn params_of(&self, list: ParamList) -> &[(NameId, TypeId)] {
        self.params.get(list.0)
    }

    /// Interns `tycon[args]`, its arguments interned first.
    pub fn intern_applied(&mut self, tycon: TypeId, args: &[TypeId]) -> TypeId {
        let args = self.intern_types(args);
        self.intern(Type::Applied(tycon, args))
    }

    /// Interns the method type of binder `binder`, parameters `params` and result `result`.
    pub fn intern_method(
        &mut self,
        binder: Binder,
        params: &[(NameId, TypeId)],
        result: TypeId,
        implicit: bool,
        contextual: bool,
    ) -> TypeId {
        let params = self.intern_params(params);
        self.intern(Type::Method { binder, params, result, implicit, contextual })
    }

    /// Interns the polymorphic type of binder `binder`, type parameters `params` and `result`.
    pub fn intern_poly(&mut self, binder: Binder, params: &[(NameId, TypeId)], result: TypeId) -> TypeId {
        let params = self.intern_params(params);
        self.intern(Type::Poly { binder, params, result })
    }

    /// Interns the type lambda of binder `binder`, parameters `params` and body `result`.
    pub fn intern_lambda(&mut self, binder: Binder, params: &[(NameId, TypeId)], result: TypeId) -> TypeId {
        let params = self.intern_params(params);
        self.intern(Type::Lambda { binder, params, result })
    }

    /// Interns `scrutinee match { cases } <: bound`.
    pub fn intern_match(&mut self, bound: TypeId, scrutinee: TypeId, cases: &[TypeId]) -> TypeId {
        let cases = self.intern_types(cases);
        self.intern(Type::Match { bound, scrutinee, cases })
    }
}

#[cfg(test)]
mod tests {
    use super::{TypeId, Types};

    #[test]
    fn interning_is_injective() {
        let mut types = Types::default();
        let (a, b) = (&[TypeId(1), TypeId(2)][..], &[TypeId(1)][..]);
        let (la, lb, le) = (types.intern_types(a), types.intern_types(b), types.intern_types(&[]));
        assert_eq!((types.intern_types(a), types.intern_types(b), types.intern_types(&[])), (la, lb, le));
        assert_ne!(la, lb);
        assert_eq!((types.types_of(la), types.types_of(lb), types.types_of(le)), (a, b, &[][..]));
    }
}
