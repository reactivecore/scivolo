// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Overloaded alternatives resolved as copies of a different type (dotty's `resolveMapped`):
//! the type after their first clause (`skipParamClause`) or without their leading implicit
//! clauses (`stripImplicit`).

use super::{Pt, Typer};
use tasty::{Binder, Kind, Symbol, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The alternative of `alts`, members of `prefix` applicable to the arguments of `proto`,
    /// that the arguments of the following clause `next` select.
    pub fn resolve_by_next_clause(
        &mut self,
        alts: &[SymbolId],
        prefix: Option<TypeId>,
        (proto, next): (usize, usize),
    ) -> Result<SymbolId, &'static str> {
        let arg_types = self.proto_arg_types(proto, alts, prefix)?;
        let mut mapped = Vec::new();
        for &alt in alts {
            let tpe = self.u.intern_type(Type::Sym(prefix, alt));
            let Some(info) = self.method_info(tpe) else { continue };
            if let Some(info) = self.skip_param_clause(info, &arg_types)? {
                mapped.push((self.mapped_alternative(alt, info, 1), alt));
            }
        }
        self.choose_mapped(&mapped, Pt::Fun(next))
    }

    /// The alternative of methods `alts`, members of `prefix`, that `pt` selects with their
    /// leading implicit clauses stripped.
    pub fn resolve_without_implicits(
        &mut self,
        alts: &[SymbolId],
        prefix: Option<TypeId>,
        pt: Pt,
    ) -> Result<SymbolId, &'static str> {
        let mut mapped = Vec::new();
        for &alt in alts {
            let tpe = self.u.intern_type(Type::Sym(prefix, alt));
            let info = self.method_info(tpe).unwrap_or(tpe);
            let original = self.mapped_alts.get(&alt).map_or(alt, |&(o, _)| o);
            let (info, clauses) = self.strip_implicit_clauses(info, original);
            mapped.push((self.mapped_alternative(alt, info, clauses), alt));
        }
        self.choose_mapped(&mapped, pt)
    }

    /// If method `s` starts with an implicit clause after its type parameters, whether that is a
    /// `using` clause.
    pub fn leading_implicit(&mut self, s: SymbolId) -> Option<bool> {
        let info = self.u.info(s);
        let info = match *self.u.types().get(info) {
            Type::Poly { result, .. } => result,
            _ => info,
        };
        match *self.u.types().get(info) {
            Type::Method { implicit: true, contextual, .. } => Some(contextual),
            _ => None,
        }
    }

    /// The original of the alternative of copies `mapped`, paired with their originals, that
    /// `pt` selects.
    fn choose_mapped(&mut self, mapped: &[(SymbolId, SymbolId)], pt: Pt) -> Result<SymbolId, &'static str> {
        let copies: Vec<SymbolId> = mapped.iter().map(|&(c, _)| c).collect();
        let back = |s: SymbolId| mapped.iter().find(|&&(c, _)| c == s).map_or(s, |&(_, a)| a);
        let chosen = self.choose(&copies, None, pt);
        self.ambiguous = std::mem::take(&mut self.ambiguous).into_iter().map(back).collect();
        chosen.map(back)
    }

    /// Type `info` of method `meth` without its leading implicit clauses, references to their
    /// parameters made wildcards bounded by their types, and the number of clauses stripped
    /// (dotty's `stripImplicit` with `resultTypeApprox`).
    fn strip_implicit_clauses(&mut self, info: TypeId, meth: SymbolId) -> (TypeId, usize) {
        match self.u.types().get(info).clone() {
            Type::Method { binder, params, result, implicit: true, .. } => {
                let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
                let wild = self.u.intern_type(Type::Wildcard(None));
                let wilds = vec![wild; names.len()];
                let mut approx = Vec::new();
                for &(_, p) in &params {
                    let p = self.u.subst(p, binder, &names, &wilds);
                    let bounds = self.u.intern_type(Type::Bounds(p, p));
                    approx.push(self.u.intern_type(Type::Wildcard(Some(bounds))));
                }
                let result = self.u.subst(result, binder, &names, &approx);
                let result = self.u.map_type(result, &mut |u, t| match *u.types().get(t) {
                    Type::Sym(None, s)
                        if u.symbol(s).kind == Kind::Param && u.symbol(s).owner == Some(meth) =>
                    {
                        names.iter().position(|n| *n == u.symbol(s).name).map(|i| approx[i])
                    }
                    _ => None,
                });
                let (result, clauses) = self.strip_implicit_clauses(result, meth);
                (result, clauses + 1)
            }
            Type::Poly { binder, params, result } => {
                let (result, clauses) = self.strip_implicit_clauses(result, meth);
                (self.flattened_poly(binder, params, result), clauses)
            }
            _ => (info, 0),
        }
    }

    /// Method type `info` after its first parameter clause applied to arguments of types
    /// `arg_types`, under its type parameters; none if `info` is no method or that type is a
    /// value of a class without `apply` members.
    fn skip_param_clause(
        &mut self,
        info: TypeId,
        arg_types: &[TypeId],
    ) -> Result<Option<TypeId>, &'static str> {
        match self.u.types().get(info).clone() {
            Type::Poly { binder, params, result } => {
                let Some(rt) = self.skip_param_clause(result, arg_types)? else { return Ok(None) };
                Ok(Some(self.flattened_poly(binder, params, rt)))
            }
            Type::Method { binder, params, result, .. } => {
                let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
                let result = self.u.subst(result, binder, &names, arg_types);
                if matches!(self.u.types().get(result), Type::Method { .. } | Type::Poly { .. }) {
                    return Ok(Some(result));
                }
                match self.u.class_symbol(result) {
                    Some(_) if self.type_members(result, "apply").is_empty() => Ok(None),
                    _ => Err("ambiguous overload resolved by an apply method of a later clause"),
                }
            }
            _ => Ok(None),
        }
    }

    /// The polymorphic type of binder `binder`, parameters `params` and result `result`, merged
    /// with `result` if that is polymorphic too, its parameters appended (dotty's
    /// `PolyType.flatten`).
    fn flattened_poly(&mut self, binder: Binder, params: Vec<(String, TypeId)>, result: TypeId) -> TypeId {
        let Type::Poly { binder: inner, params: more, result } = self.u.types().get(result).clone() else {
            return self.u.intern_type(Type::Poly { binder, params, result });
        };
        let n = params.len() as u32;
        let mut shift = |t| {
            self.u.map_type(t, &mut |u, x| match *u.types().get(x) {
                Type::Param(b, i) if b == inner => Some(u.intern_type(Type::Param(binder, n + i))),
                _ => None,
            })
        };
        let more: Vec<(String, TypeId)> =
            more.into_iter().map(|(name, bounds)| (name, shift(bounds))).collect();
        let result = shift(result);
        let params = params.into_iter().chain(more).collect();
        self.u.intern_type(Type::Poly { binder, params, result })
    }

    /// A copy of alternative `alt` of type `info`, entered nowhere, its default arguments those
    /// of the clause of `alt`'s original after the skipped ones and `skipped` more.
    fn mapped_alternative(&mut self, alt: SymbolId, info: TypeId, skipped: usize) -> SymbolId {
        let s = self.u.symbol(alt);
        let copy = self.u.define(Symbol {
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
        self.u.unenter(copy);
        self.u.set_info(copy, info);
        let (original, before) = self.mapped_alts.get(&alt).copied().unwrap_or((alt, 0));
        self.mapped_alts.insert(copy, (original, before + skipped));
        copy
    }
}
