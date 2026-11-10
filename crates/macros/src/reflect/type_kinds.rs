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
        Ok(match *self.u.ty(t) {
            Type::Applied(tycon, args) => {
                [tycon].into_iter().chain(self.u.types_of(args).iter().copied()).collect()
            }
            Type::Annotated(inner, _) | Type::This(inner) | Type::ByName(inner) | Type::Rec(_, inner) => {
                vec![inner]
            }
            Type::Sym(Some(prefix), _) | Type::TypeRef(prefix, _) | Type::TermRef(prefix, _) => vec![prefix],
            Type::Sym(None, s) => vec![self.qualifier(s)?],
            Type::Package(p) => match self.u.symbol(p).owner.filter(|_| p != self.u.root()) {
                Some(o) => vec![self.package_this_of(o)],
                None => vec![self.u.intern_type(Type::NoPrefix)],
            },
            Type::Bounds(lo, hi) | Type::And(lo, hi) | Type::Or(lo, hi, _) => vec![lo, hi],
            Type::Super(this, parent) | Type::MatchCase(this, parent) => vec![this, parent],
            Type::Alias(a) => vec![a, a],
            Type::Refined(parent, _, info) => vec![parent, info],
            Type::Match { bound, scrutinee, cases } => {
                [bound, scrutinee].into_iter().chain(self.u.types_of(cases).iter().copied()).collect()
            }
            _ => return Err("the parts of a type of another kind"),
        })
    }

    /// The type of kind `kind` (as `type_kind` numbers them) and parts `parts`, a refinement's
    /// member named `name`: the inverse of `type_parts`.
    pub fn type_of_parts(&mut self, kind: i32, parts: Vec<TypeId>, name: String) -> Fallible<TypeId> {
        let name = self.u.intern_name(&name);
        let t = match (kind, &parts[..]) {
            (5, &[lo, hi]) => Type::Bounds(lo, hi),
            (6, &[parent, info]) => Type::Refined(parent, name, info),
            (12, &[a, b]) => Type::And(a, b),
            (13, &[a, b]) => Type::Or(a, b, false),
            (17, &[bound, scrutinee, ref cases @ ..]) => {
                Type::Match { bound, scrutinee, cases: self.u.intern_types(cases) }
            }
            (20, &[pattern, body]) => Type::MatchCase(pattern, body),
            _ => return Err("a type of another kind or number of parts"),
        };
        Ok(self.u.intern_type(t))
    }

    pub fn param_num(&self, t: TypeId) -> Fallible<i32> {
        self.u.param_ref(t).map(|(_, i)| i as i32).ok_or("ParamRef.paramNum of another type")
    }

    /// Whether lambda type `t` is implicit and contextual, its number of parameters, their types
    /// and its result type; the parameters' names.
    pub fn lambda_parts(&mut self, t: TypeId) -> Answer {
        let (flags, params, result) = match *self.u.ty(t) {
            Type::Method { params, result, implicit, contextual, .. } => {
                ([implicit, contextual], params, result)
            }
            Type::Poly { params, result, .. } | Type::Lambda { params, result, .. } => {
                ([false, false], params, result)
            }
            _ => return Err("the parts of a type other than a method, polymorphic type or type lambda"),
        };
        let params = self.u.params_of(params).to_vec();
        let mut parts: Vec<i32> = flags.iter().map(|&f| f as i32).chain([params.len() as i32]).collect();
        parts.extend(params.iter().map(|&(_, t)| self.add_type(t) as i32));
        parts.push(self.add_type(result) as i32);
        let names: Vec<&str> = params.iter().map(|&(n, _)| self.u.name(n)).collect();
        Ok((parts, Some(names.join("\0").encode_utf16().collect())))
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
        match *self.u.ty(t) {
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
