// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The shapes of `TypeRepr`s for their type tests and extractors: a type's kind (1 `TypeRef`,
//! 2 `TermRef`, 3 `AppliedType`, 4 `AnnotatedType`, 5 `TypeBounds`, 6 `Refinement`, 7 `NoPrefix`,
//! 8 `MethodType`, 9 `PolyType`, 10 `ThisType`, 11 `TypeLambda`, 12 `AndType`, 13 `OrType`,
//! 14 `ByNameType`, 15 `ParamRef`, 16 `SuperType`, 17 `MatchType`, 18 `RecursiveType`,
//! 19 `RecursiveThis`, 20 `MatchCase`, 0 other) and its parts.

use super::{Answer, UNKNOWN, arg, numbers};
use crate::host::Host;
use tasty::{Type, TypeId};

impl Host {
    /// Runs type shape operation `op` on `args`.
    pub fn run_type_kind(&mut self, op: &str, args: &[i32], _: &str) -> Answer {
        let tpe = |h: &Host| h.type_of(arg(args, 0)? as u32);
        match op {
            "typeKind" => numbers(vec![self.type_kind(tpe(self)?)]),
            "typeParts" => {
                let parts = match self.u.types().get(tpe(self)?).clone() {
                    Type::Applied(tycon, args) => [tycon].into_iter().chain(args).collect(),
                    Type::Annotated(inner, _)
                    | Type::This(inner)
                    | Type::ByName(inner)
                    | Type::Rec(_, inner) => {
                        vec![inner]
                    }
                    Type::Sym(Some(prefix), _) | Type::TypeRef(prefix, _) | Type::TermRef(prefix, _) => {
                        vec![self.package_this(prefix)]
                    }
                    Type::Sym(None, s) => vec![self.qualifier(s)?],
                    Type::Package(p) => {
                        let owner = self.u.symbol(p).owner.filter(|_| p != self.u.root());
                        let this = owner.map(|o| Type::This(self.u.intern_type(Type::Sym(None, o))));
                        vec![self.u.intern_type(this.unwrap_or(Type::NoPrefix))]
                    }
                    Type::Bounds(lo, hi) | Type::And(lo, hi) | Type::Or(lo, hi, _) => vec![lo, hi],
                    Type::Super(this, parent) | Type::MatchCase(this, parent) => vec![this, parent],
                    Type::Alias(a) => vec![a, a],
                    Type::Refined(parent, _, info) => vec![parent, info],
                    Type::Match { bound, scrutinee, cases } => {
                        [bound, scrutinee].into_iter().chain(cases).collect()
                    }
                    _ => return Err("the parts of a type of another kind"),
                };
                numbers(parts.into_iter().map(|p| self.add_type(p) as i32).collect())
            }
            "paramNum" => match *self.u.types().get(tpe(self)?) {
                Type::Param(_, i) => numbers(vec![i as i32]),
                _ => Err("ParamRef.paramNum of another type"),
            },
            "lambdaParts" => {
                let (flags, params, result) = match self.u.types().get(tpe(self)?).clone() {
                    Type::Method { params, result, implicit, contextual, .. } => {
                        ([implicit, contextual], params, result)
                    }
                    Type::Poly { params, result, .. } | Type::Lambda { params, result, .. } => {
                        ([false, false], params, result)
                    }
                    _ => {
                        return Err(
                            "the parts of a type other than a method, polymorphic type or type lambda",
                        );
                    }
                };
                let mut parts: Vec<i32> =
                    flags.iter().map(|&f| f as i32).chain([params.len() as i32]).collect();
                parts.extend(params.iter().map(|&(_, t)| self.add_type(t) as i32));
                parts.push(self.add_type(result) as i32);
                let names: Vec<&str> = params.iter().map(|(n, _)| n.as_str()).collect();
                Ok((parts, Some(names.join("\0"))))
            }
            _ => Err(UNKNOWN),
        }
    }

    /// Prefix `t` of a member, the `this` of the package if it is a package (dotty's
    /// `owner.thisType` of a package member).
    fn package_this(&mut self, t: TypeId) -> TypeId {
        match *self.u.types().get(t) {
            Type::Package(p) => {
                let class = self.u.intern_type(Type::Sym(None, p));
                self.u.intern_type(Type::This(class))
            }
            _ => t,
        }
    }

    /// `t` with the `this` of each package `package_this` made the package's path again, as the
    /// universe's operations take it.
    pub fn as_paths(&mut self, t: TypeId) -> TypeId {
        self.u.map_type(t, &mut |u, part| match *u.types().get(part) {
            Type::This(c) => match *u.types().get(c) {
                Type::Sym(None, p) if u.symbol(p).kind == tasty::Kind::Package => {
                    Some(u.intern_type(Type::Package(p)))
                }
                _ => None,
            },
            _ => None,
        })
    }

    fn type_kind(&self, t: TypeId) -> i32 {
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
