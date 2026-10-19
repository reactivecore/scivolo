//! Erasure of applied types: arrays (dotty's `isGenericArrayElement`), tuples, unions and
//! intersections written as `|[A, B]`/`&[A, B]`, and applied aliases, whose arguments are
//! substituted into their right-hand side.

use super::special::tuple;
use super::{Erased, object};
use crate::types::{Type, TypeId};
use crate::{Kind, Universe};

impl Universe {
    pub fn erase_applied(&mut self, tycon: TypeId, args: &[TypeId]) -> Erased {
        match self.class_name(tycon).as_deref() {
            Some("scala.Array") => match args.first() {
                Some(&elem) => self.erase_array(elem),
                None => object(),
            },
            Some("scala.*:") => match self.tuple_arity(args) {
                Some(n) => tuple(n),
                None => Erased::Class("scala.Product".into()),
            },
            Some("scala.|") if args.len() == 2 => self.erase_or(args[0], args[1]),
            Some("scala.&") if args.len() == 2 => self.erase_and(args[0], args[1]),
            Some(_) => match self.generic_underlying(tycon, args) {
                Some(arg) => match self.erase_boxed(arg) {
                    Erased::Prim(c) => super::special::boxed(c),
                    e => e,
                },
                None => self.erase(tycon),
            },
            None => match self.expand(tycon, args) {
                Some(t) => self.erase(t),
                None => self.erase(tycon),
            },
        }
    }

    /// The argument of `tycon[args]`, a value class whose underlying value has the type of one of
    /// its type parameters, for that parameter (dotty's `eraseDerivedValueClass`).
    fn generic_underlying(&mut self, tycon: TypeId, args: &[TypeId]) -> Option<TypeId> {
        let class = self.class_symbol(tycon)?;
        let value = self.underlying(class)?;
        let info = self.info(value);
        let param = self.referenced(info).filter(|&p| self.symbol(p).kind == Kind::TypeParam)?;
        let i = self.type_params(class).iter().position(|&p| p == param)?;
        args.get(i).copied()
    }

    /// `Array[elem]`: `Object` for a generic element or a wildcard bounded by `Unit`, else an
    /// array of the erased element (`BoxedUnit` for `Unit`, `Object` for `Nothing` and `Null`).
    fn erase_array(&mut self, elem: TypeId) -> Erased {
        let wildcard = matches!(self.types().get(elem), Type::Bounds(..));
        if self.is_generic(elem) {
            return object();
        }
        Erased::Array(Box::new(match self.erase_boxed(elem) {
            Erased::Unit if wildcard => return object(),
            Erased::Unit => Erased::Class("scala.runtime.BoxedUnit".into()),
            Erased::Nothing | Erased::Null => object(),
            e => e,
        }))
    }

    /// The number of elements of `h *: t` if `t` is a tuple of known length.
    fn tuple_arity(&mut self, args: &[TypeId]) -> Option<usize> {
        let [_, tail] = args else { return None };
        match self.types().get(*tail).clone() {
            Type::Applied(c, a) if self.class_name(c).as_deref() == Some("scala.*:") => {
                Some(1 + self.tuple_arity(&a)?)
            }
            _ if self.class_name(*tail).as_deref() == Some("scala.Tuple$package.EmptyTuple") => Some(1),
            Type::TypeRef(_, n) if n == "EmptyTuple" => Some(1),
            Type::Sym(_, s)
                if self.symbol(s).name == "EmptyTuple" && self.full_name(s).starts_with("scala.") =>
            {
                Some(1)
            }
            _ => None,
        }
    }

    /// `tycon[args]` with `tycon` an alias of a type lambda: its body as seen from the prefix of
    /// `tycon`, arguments substituted.
    pub fn expand(&mut self, tycon: TypeId, args: &[TypeId]) -> Option<TypeId> {
        self.applied_info(tycon, args, false)
    }

    /// `expand`, or for `tycon` an abstract type bounded by a type lambda, that lambda's body
    /// (dotty's `superType` of an applied type).
    pub fn expand_upper(&mut self, tycon: TypeId, args: &[TypeId]) -> Option<TypeId> {
        self.applied_info(tycon, args, true)
    }

    fn applied_info(&mut self, tycon: TypeId, args: &[TypeId], upper: bool) -> Option<TypeId> {
        let (prefix, alias) = match self.types().get(tycon).clone() {
            Type::Sym(p, s) => (p, Some(s)),
            Type::TypeRef(p, n) => (Some(p), self.resolve(p, &n, true)),
            _ => (None, None),
        };
        let alias = alias?;
        let kind = self.symbol(alias).kind;
        if kind != Kind::Type && !(upper && kind == Kind::TypeParam) {
            return None;
        }
        let info = self.info(alias);
        let info = match *self.types().get(info) {
            Type::Bounds(_, hi) if upper => hi,
            _ => info,
        };
        let owner = self.symbol(alias).owner;
        let info = match (prefix, owner) {
            (Some(p), Some(o)) if self.symbol(o).kind == Kind::Class => self.as_seen_from(info, p, o),
            _ => info,
        };
        let Type::Lambda { binder, params, result } = self.types().get(info).clone() else { return None };
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        Some(self.subst(result, binder, &names, args))
    }

    /// `t` with the parameters of lambda `binder`, named `names`, replaced by `args`.
    pub fn subst(&mut self, t: TypeId, binder: crate::Binder, names: &[String], args: &[TypeId]) -> TypeId {
        self.map_type(t, &mut |u, x| match u.types().get(x) {
            Type::Local(b, n) if b.0 == binder.0 => {
                names.iter().position(|m| m == n).and_then(|i| args.get(i).copied())
            }
            Type::Param(b, i) if *b == binder => args.get(*i as usize).copied(),
            _ => None,
        })
    }

    /// The symbol a `Sym` or `TypeRef` refers to.
    pub fn referenced(&mut self, t: TypeId) -> Option<crate::SymbolId> {
        match self.types().get(t).clone() {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(p, n) => self.resolve(p, &n, true),
            _ => None,
        }
    }
}
