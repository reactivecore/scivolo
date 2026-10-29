// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The shapes of `TypeRepr`s for their type tests and extractors: a type's kind (1 `TypeRef`,
//! 2 `TermRef`, 3 `AppliedType`, 4 `AnnotatedType`, 5 `TypeBounds`, 6 `Refinement`, 7 `NoPrefix`,
//! 8 `MethodType`, 9 `PolyType`, 10 `ThisType`, 11 `TypeLambda`, 12 `AndType`, 13 `OrType`,
//! 14 `ByNameType`, 15 `ParamRef`, 16 `SuperType`, 17 `MatchType`, 18 `RecursiveType`,
//! 19 `RecursiveThis`, 20 `MatchCase`, 0 other) and its parts.

use super::{Answer, Fallible};
use crate::host::Host;
use tasty::{SymbolId, Type, TypeId};

impl Host {
    pub fn type_parts(&mut self, t: TypeId) -> Fallible<Vec<TypeId>> {
        Ok(match self.u.types().get(t).clone() {
            Type::Applied(tycon, args) => [tycon].into_iter().chain(args).collect(),
            Type::Annotated(inner, _) | Type::This(inner) | Type::ByName(inner) | Type::Rec(_, inner) => {
                vec![inner]
            }
            Type::Sym(Some(prefix), _) | Type::TypeRef(prefix, _) | Type::TermRef(prefix, _) => {
                vec![self.package_this(prefix)]
            }
            Type::Sym(None, s) => vec![self.qualifier(s)?],
            Type::Package(p) => match self.u.symbol(p).owner.filter(|_| p != self.u.root()) {
                Some(o) => vec![self.package_this_of(o)],
                None => vec![self.u.intern_type(Type::NoPrefix)],
            },
            Type::Bounds(lo, hi) | Type::And(lo, hi) | Type::Or(lo, hi, _) => vec![lo, hi],
            Type::Super(this, parent) | Type::MatchCase(this, parent) => vec![this, parent],
            Type::Alias(a) => vec![a, a],
            Type::Refined(parent, _, info) => vec![parent, info],
            Type::Match { bound, scrutinee, cases } => [bound, scrutinee].into_iter().chain(cases).collect(),
            _ => return Err("the parts of a type of another kind"),
        })
    }

    pub fn param_num(&self, t: TypeId) -> Fallible<i32> {
        match *self.u.types().get(t) {
            Type::Param(_, i) => Ok(i as i32),
            _ => Err("ParamRef.paramNum of another type"),
        }
    }

    /// Whether lambda type `t` is implicit and contextual, its number of parameters, their types
    /// and its result type; the parameters' names.
    pub fn lambda_parts(&mut self, t: TypeId) -> Answer {
        let (flags, params, result) = match self.u.types().get(t).clone() {
            Type::Method { params, result, implicit, contextual, .. } => {
                ([implicit, contextual], params, result)
            }
            Type::Poly { params, result, .. } | Type::Lambda { params, result, .. } => {
                ([false, false], params, result)
            }
            _ => return Err("the parts of a type other than a method, polymorphic type or type lambda"),
        };
        let mut parts: Vec<i32> = flags.iter().map(|&f| f as i32).chain([params.len() as i32]).collect();
        parts.extend(params.iter().map(|&(_, t)| self.add_type(t) as i32));
        parts.push(self.add_type(result) as i32);
        let names: Vec<&str> = params.iter().map(|(n, _)| n.as_str()).collect();
        Ok((parts, Some(names.join("\0"))))
    }

    /// Prefix `t` of a member, the `this` of the package if it is a package (dotty's
    /// `owner.thisType` of a package member).
    fn package_this(&mut self, t: TypeId) -> TypeId {
        match *self.u.types().get(t) {
            Type::Package(p) => self.package_this_of(p),
            _ => t,
        }
    }

    /// The `this` of package `p`, which the operations other than type tests and extractors
    /// take as the package's path.
    pub fn package_this_of(&mut self, p: SymbolId) -> TypeId {
        let class = self.u.intern_type(Type::Sym(None, p));
        let this = self.u.intern_type(Type::This(class));
        let path = self.u.intern_type(Type::Package(p));
        self.package_paths.insert(this, path);
        this
    }

    pub fn type_kind(&self, t: TypeId) -> i32 {
        match *self.u.types().get(t) {
            Type::Sym(_, s) if self.u.symbol(s).kind.is_term() => 2,
            Type::Sym(..) | Type::TypeRef(..) => 1,
            Type::TermRef(..) | Type::Package(_) => 2,
            Type::Applied(..) => 3,
            Type::Annotated(..) => 4,
            Type::Bounds(..) | Type::Alias(_) => 5,
            Type::Refined(..) => 6,
            Type::NoPrefix => 7,
            Type::Method { .. } => 8,
            Type::Poly { .. } => 9,
            Type::This(_) => 10,
            Type::Lambda { .. } => 11,
            Type::And(..) => 12,
            Type::Or(..) => 13,
            Type::ByName(_) => 14,
            Type::Param(..) => 15,
            Type::Super(..) => 16,
            Type::Match { .. } => 17,
            Type::Rec(..) => 18,
            Type::RecThis(_) => 19,
            Type::MatchCase(..) => 20,
            _ => 0,
        }
    }
}
