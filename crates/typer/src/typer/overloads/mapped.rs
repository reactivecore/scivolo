// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Overloaded alternatives resolved as copies of a different type (dotty's `resolveMapped`):
//! the type after their first clause (`skipParamClause`) or without their leading implicit
//! clauses (`stripImplicit`).

use super::{Resolution, Unchosen};
use crate::typer::{Pt, Typer};
use tasty::{Binder, Kind, NameId, Symbol, SymbolId, Type, TypeId, Universe, nme};

/// An alternative's copy, paired with the alternative.
type Mapped = (SymbolId, SymbolId);

impl Resolution {
    /// Chooses the alternative of `alts`, applicable to the arguments of `proto`, that the
    /// arguments of the following clause `next` select, as `f(1)("a")` among
    /// `def f(x: Int)(y: Int)` and `def f(x: Int)(y: String)`.
    pub fn resolve_by_next_clause(
        self,
        t: &mut Typer,
        alts: &[SymbolId],
        (proto, next): (usize, usize),
    ) -> Result<SymbolId, Unchosen> {
        let arg_types = self.proto_arg_types(t, proto, alts)?;
        let mut mapped = Vec::new();
        for &alt in alts {
            let tpe = t.u.intern_type(Type::Sym(self.prefix, alt));
            let Some(info) = t.method_info(tpe) else { continue };
            if let Some(info) = skip_param_clause(t, info, &arg_types)? {
                mapped.push((mapped_alternative(t, alt, info, 1), alt));
            }
        }
        choose_mapped(t, &mapped, Pt::Fun(next))
    }

    /// Chooses the alternative of methods `alts` that the expected type selects with their
    /// leading implicit clauses stripped, as `f(1)` among `def f(x: Int)(using Ord[Int])` and
    /// `def f(x: String)`.
    pub fn resolve_without_implicits(self, t: &mut Typer, alts: &[SymbolId]) -> Result<SymbolId, Unchosen> {
        let mut mapped = Vec::new();
        for &alt in alts {
            let tpe = t.u.intern_type(Type::Sym(self.prefix, alt));
            let info = t.method_info(tpe).unwrap_or(tpe);
            let original = t.mapped_alts.get(&alt).map_or(alt, |&(o, _)| o);
            let (info, clauses) = strip_implicit_clauses(t.u, info, original);
            mapped.push((mapped_alternative(t, alt, info, clauses), alt));
        }
        choose_mapped(t, &mapped, self.pt)
    }
}

/// Checks whether method `s` starts with an implicit clause after its type parameters, and
/// whether that is a `using` clause.
pub fn leading_implicit(u: &mut Universe, s: SymbolId) -> Option<bool> {
    let info = u.info(s);
    let info = u.strip_poly(info);
    u.method(info).filter(|m| m.implicit).map(|m| m.contextual)
}

/// Chooses the original of the alternative of copies `mapped` that `pt` selects, in a nested
/// resolution.
fn choose_mapped(t: &mut Typer, mapped: &[Mapped], pt: Pt) -> Result<SymbolId, Unchosen> {
    let copies: Vec<SymbolId> = mapped.iter().map(|&(c, _)| c).collect();
    let back = |s: SymbolId| mapped.iter().find(|&&(c, _)| c == s).map_or(s, |&(_, a)| a);
    let chosen = Resolution { prefix: None, pt }.choose(t, &copies);
    chosen
        .map(back)
        .map_err(|e| Unchosen { ambiguous: e.ambiguous.map(|a| a.into_iter().map(back).collect()), ..e })
}

/// Strips the leading implicit clauses off type `info` of method `meth`, references to their
/// parameters made wildcards bounded by their types, and counts the clauses stripped (dotty's
/// `stripImplicit` with `resultTypeApprox`).
pub fn strip_implicit_clauses(u: &mut Universe, info: TypeId, meth: SymbolId) -> (TypeId, usize) {
    match *u.ty(info) {
        Type::Method { binder, params, result, implicit: true, .. } => {
            let names: Vec<NameId> = u.params_of(params).iter().map(|&(n, _)| n).collect();
            let wild = u.intern_type(Type::Wildcard(None));
            let wilds = vec![wild; names.len()];
            let mut approx = Vec::new();
            for i in 0..names.len() {
                let p = u.params_of(params)[i].1;
                let p = u.subst(p, binder, &names, &wilds);
                let bounds = u.intern_type(Type::Bounds(p, p));
                approx.push(u.intern_type(Type::Wildcard(Some(bounds))));
            }
            let result = u.subst(result, binder, &names, &approx);
            let result = u.map_type(result, &mut |u, t| match *u.ty(t) {
                Type::Sym(None, s) if u.symbol(s).kind == Kind::Param && u.symbol(s).owner == Some(meth) => {
                    names.iter().position(|&n| u.name(n) == u.symbol(s).name).map(|i| approx[i])
                }
                _ => None,
            });
            let (result, clauses) = strip_implicit_clauses(u, result, meth);
            (result, clauses + 1)
        }
        Type::Poly { binder, params, result } => {
            let (result, clauses) = strip_implicit_clauses(u, result, meth);
            let params = u.params_of(params).to_vec();
            (flattened_poly(u, binder, params, result), clauses)
        }
        _ => (info, 0),
    }
}

/// Gives method type `info` after its first parameter clause applied to arguments of types
/// `arg_types`, under its type parameters; none if `info` is no method or that type is a value
/// of a class without `apply` members.
fn skip_param_clause(
    t: &mut Typer,
    info: TypeId,
    arg_types: &[TypeId],
) -> Result<Option<TypeId>, &'static str> {
    match *t.u.ty(info) {
        Type::Poly { binder, params, result } => {
            let Some(rt) = skip_param_clause(t, result, arg_types)? else { return Ok(None) };
            let params = t.u.params_of(params).to_vec();
            Ok(Some(flattened_poly(t.u, binder, params, rt)))
        }
        Type::Method { binder, params, result, .. } => {
            let names: Vec<NameId> = t.u.params_of(params).iter().map(|&(n, _)| n).collect();
            let result = t.u.subst(result, binder, &names, arg_types);
            if matches!(t.u.ty(result), Type::Method { .. } | Type::Poly { .. }) {
                return Ok(Some(result));
            }
            match t.u.class_symbol(result) {
                Some(_) if t.type_members(result, nme::APPLY).is_empty() => Ok(None),
                _ => Err("ambiguous overload resolved by an apply method of a later clause"),
            }
        }
        _ => Ok(None),
    }
}

/// Gives the polymorphic type of binder `binder`, parameters `params` and result `result`,
/// merged with `result` if that is polymorphic too, its parameters appended (dotty's
/// `PolyType.flatten`).
fn flattened_poly(u: &mut Universe, binder: Binder, params: Vec<(NameId, TypeId)>, result: TypeId) -> TypeId {
    let Some((inner, more, result)) = u.poly(result).map(|p| (p.binder, p.params.to_vec(), p.result)) else {
        return u.intern_poly(binder, &params, result);
    };
    let n = params.len() as u32;
    let mut shift = |t| {
        u.map_type(t, &mut |u, x| {
            let (_, i) = u.param_ref(x).filter(|&(b, _)| b == inner)?;
            Some(u.intern_type(Type::Param(binder, n + i)))
        })
    };
    let more: Vec<(NameId, TypeId)> = more.into_iter().map(|(name, bounds)| (name, shift(bounds))).collect();
    let result = shift(result);
    let params: Vec<_> = params.into_iter().chain(more).collect();
    u.intern_poly(binder, &params, result)
}

/// Makes a copy of alternative `alt` of type `info`, entered nowhere, its default arguments
/// those of the clause of `alt`'s original after the skipped ones and `skipped` more.
fn mapped_alternative(t: &mut Typer, alt: SymbolId, info: TypeId, skipped: usize) -> SymbolId {
    let s = t.u.symbol(alt);
    let copy = t.u.define(Symbol {
        name: s.name.clone(),
        kind: s.kind,
        owner: s.owner,
        flags: s.flags,
        target: s.target.clone(),
        is_extern: s.is_extern,
        derived: s.derived,
        decls: Vec::new(),
        tree: None,
    });
    t.u.unenter(copy);
    t.u.set_info(copy, info);
    let (original, before) = t.mapped_alts.get(&alt).copied().unwrap_or((alt, 0));
    t.mapped_alts.insert(copy, (original, before + skipped));
    copy
}
