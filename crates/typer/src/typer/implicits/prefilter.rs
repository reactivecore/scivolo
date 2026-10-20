// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//! Rejecting an implicit candidate before it is referenced and typed: one with explicit
//! parameters, as `try_implicit` does, or one whose declared result type cannot conform by the
//! classes it and its type arguments refer to, type parameters and uninstantiated type
//! variables taken as wildcards within their bounds (after dotty's `isCompatible` in
//! `candidateKind`, `wildApprox`).

use super::super::Typer;
use crate::symbols::Completion;
use tasty::{Kind, SymbolId, Type, TypeId, TypeVars, VarId};

/// How deep type arguments are compared.
const DEPTH: u32 = 4;
/// Classes whose types conform to or from others other than by their base classes.
const SPECIAL: [&str; 9] = [
    "scala.Any",
    "scala.AnyKind",
    "scala.AnyVal",
    "scala.Matchable",
    "java.lang.Object",
    "scala.Singleton",
    "scala.Nothing",
    "scala.Null",
    "scala.Product",
];

impl Typer<'_> {
    /// Whether the value of implicit `sym` cannot conform to `pt`: it takes explicit parameters,
    /// or the class of its declared result type does not derive from that of `pt`, or a type
    /// argument of its base type at that class cannot conform to the corresponding one of `pt`.
    pub fn cannot_fit(&mut self, sym: SymbolId, pt: TypeId) -> bool {
        let mut t = self.u.info(sym);
        loop {
            t = match *self.u.types().get(t) {
                Type::Poly { result, .. }
                | Type::Method { result, implicit: true, .. }
                | Type::ByName(result) => result,
                Type::Method { .. } => return true,
                _ => break,
            };
        }
        let (Some((ca, _)), Some((cb, ba))) = (self.nominal(t), self.nominal(pt)) else { return false };
        if !self.related(ca, cb) {
            return true;
        }
        let t = self.u.dealias(t);
        let Some((_, aa)) = self.u.base_type(t, cb).and_then(|b| self.nominal(b)) else { return false };
        let variances = self.u.variances(cb);
        aa.len() == ba.len()
            && aa.iter().zip(&ba).enumerate().any(|(i, (&x, &y))| {
                match (self.open_var(y), variances.get(i)) {
                    (Some(v), Some(&variance)) => self.outside_bounds(x, v, variance),
                    (Some(_), None) => false,
                    (None, _) => self.unrelated(x, y, DEPTH),
                }
            })
    }

    /// Whether `x` cannot be an argument of variance `variance` for type variable `v` within
    /// its bounds.
    fn outside_bounds(&mut self, x: TypeId, v: VarId, variance: i8) -> bool {
        let Some((cx, _)) = self.nominal(x) else { return false };
        let (lo, hi) = self.constraint.bounds(v);
        if variance <= 0
            && let Some((cl, _)) = self.nominal(lo)
            && !self.related(cl, cx)
        {
            return true;
        }
        variance >= 0 && self.nominal(hi).is_some_and(|(ch, _)| !self.related(cx, ch))
    }

    /// Whether `a` and `b` cannot be related by conformance in either direction.
    fn unrelated(&mut self, a: TypeId, b: TypeId, depth: u32) -> bool {
        let (Some((ca, aa)), Some((cb, ba))) = (self.nominal(a), self.nominal(b)) else { return false };
        if ca != cb {
            return !self.related(ca, cb) && !self.related(cb, ca);
        }
        depth > 0
            && aa.len() == ba.len()
            && aa.iter().zip(&ba).any(|(&x, &y)| self.unrelated(x, y, depth - 1))
    }

    /// Whether a type of class `sub` may conform to one of class `sup`: `sub` derives from
    /// `sup`, or both are tuple classes other than two `TupleN` of different arities.
    fn related(&mut self, sub: SymbolId, sup: SymbolId) -> bool {
        match (tuple_class(&self.u.full_name(sub)), tuple_class(&self.u.full_name(sup))) {
            (Some(Some(n)), Some(Some(m))) if n != m => false,
            (Some(_), Some(_)) => true,
            _ => self.u.derives_from(sub, sup),
        }
    }

    /// The type variable `t` is, if not instantiated.
    fn open_var(&self, t: TypeId) -> Option<VarId> {
        match *self.u.types().get(t) {
            Type::Var(v) if self.constraint.instance(v).is_none() => Some(v),
            _ => None,
        }
    }

    /// The class `t` refers to and its type arguments, if it is a class other than a special one
    /// whose base classes are known.
    fn nominal(&mut self, t: TypeId) -> Option<(SymbolId, Vec<TypeId>)> {
        let t = self.u.dealias(t);
        let (tycon, args) = match self.u.types().get(t) {
            Type::Applied(tycon, args) => (*tycon, args.clone()),
            _ => (t, Vec::new()),
        };
        let c = match *self.u.types().get(tycon) {
            Type::Sym(_, s) => s,
            Type::TypeRef(..) => self.u.resolve_ref(tycon)?,
            _ => return None,
        };
        let full = self.u.full_name(c);
        let special = SPECIAL.contains(&full.as_str()) || full.starts_with("scala.ContextFunction");
        let known = matches!(self.syms.completions.get(&c), None | Some(Completion::Done))
            && !self.syms.incomplete.contains(&c);
        (self.u.symbol(c).kind == Kind::Class && !special && known).then_some((c, args))
    }
}

/// Whether `full` names a tuple class, with its arity if it is a `TupleN`.
fn tuple_class(full: &str) -> Option<Option<u32>> {
    let name = full.strip_prefix("scala.")?;
    match name {
        "Tuple" | "*:" | "EmptyTuple" | "EmptyTuple$" | "NonEmptyTuple" => Some(None),
        _ => name.strip_prefix("Tuple")?.parse().ok().map(Some),
    }
}
