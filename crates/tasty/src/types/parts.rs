// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Borrowed views of the parts of types, in the style of dotty's extractors.

use super::{Binder, NameId, ParamList, Type, TypeId, TypeList, VarId};
use crate::{SymbolId, Universe};

/// The parts a `Method`, `Poly` or `Lambda` type binds; `implicit` and `contextual` are false
/// for the latter two.
#[derive(Clone, Copy)]
pub struct Binding<'a> {
    pub binder: Binder,
    pub params: &'a [(NameId, TypeId)],
    pub result: TypeId,
    pub implicit: bool,
    pub contextual: bool,
}

impl Binding<'_> {
    pub fn param_types(&self) -> Vec<TypeId> {
        self.params.iter().map(|(_, t)| *t).collect()
    }

    pub fn names(&self) -> Vec<NameId> {
        self.params.iter().map(|&(n, _)| n).collect()
    }
}

impl Universe {
    /// The type `t` stands for.
    pub fn ty(&self, t: TypeId) -> &Type {
        self.types().get(t)
    }

    /// The text of name `id`.
    pub fn name(&self, id: NameId) -> &str {
        self.types().name(id)
    }

    /// The types of list `list`.
    pub fn types_of(&self, list: TypeList) -> &[TypeId] {
        self.types().types_of(list)
    }

    /// The parameters of list `list`.
    pub fn params_of(&self, list: ParamList) -> &[(NameId, TypeId)] {
        self.types().params_of(list)
    }

    /// The prefix and symbol of a `Sym` type.
    pub fn sym_ref(&self, t: TypeId) -> Option<(Option<TypeId>, SymbolId)> {
        match *self.ty(t) {
            Type::Sym(p, s) => Some((p, s)),
            _ => None,
        }
    }

    /// The symbol of a `Sym` type.
    pub fn sym_of(&self, t: TypeId) -> Option<SymbolId> {
        self.sym_ref(t).map(|(_, s)| s)
    }

    /// The prefix and name of a `TypeRef` type.
    pub fn type_ref(&self, t: TypeId) -> Option<(TypeId, NameId)> {
        match *self.ty(t) {
            Type::TypeRef(p, n) => Some((p, n)),
            _ => None,
        }
    }

    /// The prefix and name of a `TermRef` type.
    pub fn term_ref(&self, t: TypeId) -> Option<(TypeId, NameId)> {
        match *self.ty(t) {
            Type::TermRef(p, n) => Some((p, n)),
            _ => None,
        }
    }

    /// The class type of a `This` type.
    pub fn this_of(&self, t: TypeId) -> Option<TypeId> {
        match *self.ty(t) {
            Type::This(c) => Some(c),
            _ => None,
        }
    }

    /// The lower and upper bound of a `Bounds` type.
    pub fn bounds(&self, t: TypeId) -> Option<(TypeId, TypeId)> {
        match *self.ty(t) {
            Type::Bounds(lo, hi) => Some((lo, hi)),
            _ => None,
        }
    }

    /// The aliased type of an `Alias` type.
    pub fn type_alias(&self, t: TypeId) -> Option<TypeId> {
        match *self.ty(t) {
            Type::Alias(a) => Some(a),
            _ => None,
        }
    }

    /// The result type of a `ByName` type.
    pub fn by_name(&self, t: TypeId) -> Option<TypeId> {
        match *self.ty(t) {
            Type::ByName(r) => Some(r),
            _ => None,
        }
    }

    /// The underlying type and the annotation's class type of an `Annotated` type.
    pub fn annotated(&self, t: TypeId) -> Option<(TypeId, TypeId)> {
        match *self.ty(t) {
            Type::Annotated(inner, annot) => Some((inner, annot)),
            _ => None,
        }
    }

    /// The binder and index of a `Param` type.
    pub fn param_ref(&self, t: TypeId) -> Option<(Binder, u32)> {
        match *self.ty(t) {
            Type::Param(b, i) => Some((b, i)),
            _ => None,
        }
    }

    /// The type variable of a `Var` type.
    pub fn var(&self, t: TypeId) -> Option<VarId> {
        match *self.ty(t) {
            Type::Var(v) => Some(v),
            _ => None,
        }
    }

    pub fn method(&self, t: TypeId) -> Option<Binding<'_>> {
        match *self.ty(t) {
            Type::Method { binder, params, result, implicit, contextual } => {
                Some(Binding { binder, params: self.params_of(params), result, implicit, contextual })
            }
            _ => None,
        }
    }

    pub fn poly(&self, t: TypeId) -> Option<Binding<'_>> {
        match *self.ty(t) {
            Type::Poly { binder, params, result } => Some(plain(binder, self.params_of(params), result)),
            _ => None,
        }
    }

    pub fn lambda(&self, t: TypeId) -> Option<Binding<'_>> {
        match *self.ty(t) {
            Type::Lambda { binder, params, result } => Some(plain(binder, self.params_of(params), result)),
            _ => None,
        }
    }

    /// The type constructor and arguments of an `Applied` type.
    pub fn applied(&self, t: TypeId) -> Option<(TypeId, &[TypeId])> {
        match *self.ty(t) {
            Type::Applied(tycon, args) => Some((tycon, self.types_of(args))),
            _ => None,
        }
    }

    /// The result of a `Poly` type, else `t` (one layer of dotty's `stripPoly`).
    pub fn strip_poly(&self, t: TypeId) -> TypeId {
        match *self.ty(t) {
            Type::Poly { result, .. } => result,
            _ => t,
        }
    }
}

fn plain(binder: Binder, params: &[(NameId, TypeId)], result: TypeId) -> Binding<'_> {
    Binding { binder, params, result, implicit: false, contextual: false }
}
