//! Erasure of applied types: arrays (dotty's `isGenericArrayElement`), tuples, unions and
//! intersections written as `|[A, B]`/`&[A, B]`, and applied aliases, whose arguments are
//! substituted into their right-hand side.

use super::special::{UNBOUNDED, special, tuple};
use super::{Erased, object};
use crate::types::{Type, TypeId};
use crate::{Kind, Universe};

/// The kind of JVM array that can hold every value of a type (dotty's `arrayUpperBound`).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Holder {
    None,
    Object,
    Prim(char),
}

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
            _ => None,
        }
    }

    /// `tycon[args]` with `tycon` an alias of a type lambda: its body, arguments substituted.
    pub fn expand(&mut self, tycon: TypeId, args: &[TypeId]) -> Option<TypeId> {
        let alias = match self.types().get(tycon).clone() {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(p, n) => self.resolve(p, &n, true),
            _ => None,
        }?;
        if self.symbol(alias).kind != Kind::Type {
            return None;
        }
        let info = self.info(alias);
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

    /// Whether an array of `t` erases to `Object`: no JVM array holds all its values.
    pub fn is_generic(&mut self, t: TypeId) -> bool {
        match self.types().get(t).clone() {
            Type::Param(..) | Type::Local(..) => true,
            Type::Bounds(_, hi) => self.holder(hi) == Holder::None,
            Type::Annotated(u, _) | Type::Refined(u, _, _) | Type::Rec(_, u) => self.is_generic(u),
            Type::And(a, b) => self.is_generic(a) && self.is_generic(b),
            Type::Or(a, b) => self.is_generic(a) || self.is_generic(b),
            Type::Sym(..) | Type::TypeRef(..) => match self.referenced(t) {
                Some(s) if matches!(self.symbol(s).kind, Kind::Type | Kind::TypeParam) => {
                    let info = self.info(s);
                    match self.types().get(info).clone() {
                        Type::Bounds(_, hi) => self.holder(hi) == Holder::None,
                        Type::Lambda { .. } => false,
                        _ => self.is_generic(info),
                    }
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// The kind of array that can hold every value of type `t`.
    fn holder(&mut self, t: TypeId) -> Holder {
        match self.types().get(t).clone() {
            Type::Annotated(u, _) | Type::Refined(u, _, _) | Type::Rec(_, u) | Type::Bounds(_, u) => {
                self.holder(u)
            }
            Type::Applied(c, _) => self.holder(c),
            Type::And(a, b) => match (self.holder(a), self.holder(b)) {
                (x, y) if x == y => x,
                (Holder::None, y) => y,
                (x, _) => x,
            },
            Type::Or(a, b) => match (self.holder(a), self.holder(b)) {
                (x, y) if x == y => x,
                _ => Holder::None,
            },
            Type::Sym(..) | Type::TypeRef(..) => {
                if let Some(c) = self.class_name(t) {
                    return match special(&c) {
                        _ if UNBOUNDED.contains(&c.as_str()) => Holder::None,
                        Some(Erased::Prim(p)) => Holder::Prim(p),
                        _ => Holder::Object,
                    };
                }
                let Some(s) = self.referenced(t) else { return Holder::None };
                let info = self.info(s);
                if matches!(self.types().get(info), Type::Lambda { .. }) {
                    Holder::None
                } else {
                    self.holder(info)
                }
            }
            _ => Holder::None,
        }
    }

    /// The symbol a `Sym` or `TypeRef` refers to.
    fn referenced(&mut self, t: TypeId) -> Option<crate::SymbolId> {
        match self.types().get(t).clone() {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(p, n) => self.resolve(p, &n, true),
            _ => None,
        }
    }
}
