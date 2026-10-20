// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Overloaded alternatives resolved by the argument clause after the first (dotty's
//! `resolveMapped` with `skipParamClause`): each replaced by a copy of the type it has after
//! its first clause.

use super::{Pt, Typer};
use tasty::{Symbol, SymbolId, Type, TypeId};

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
                mapped.push((self.mapped_alternative(alt, info), alt));
            }
        }
        let copies: Vec<SymbolId> = mapped.iter().map(|&(c, _)| c).collect();
        let back = |s: SymbolId| mapped.iter().find(|&&(c, _)| c == s).map_or(s, |&(_, a)| a);
        let chosen = self.choose(&copies, None, Pt::Fun(next));
        self.ambiguous = std::mem::take(&mut self.ambiguous).into_iter().map(back).collect();
        chosen.map(back)
    }

    /// Method type `info` after its first parameter clause applied to arguments of types
    /// `arg_types`, under its type parameters; none if `info` is no method.
    fn skip_param_clause(
        &mut self,
        info: TypeId,
        arg_types: &[TypeId],
    ) -> Result<Option<TypeId>, &'static str> {
        match self.u.types().get(info).clone() {
            Type::Poly { binder, params, result } => {
                let Some(rt) = self.skip_param_clause(result, arg_types)? else { return Ok(None) };
                if matches!(self.u.types().get(rt), Type::Poly { .. }) {
                    return Err("ambiguous overload resolved by type arguments of a later clause");
                }
                Ok(Some(self.u.intern_type(Type::Poly { binder, params, result: rt })))
            }
            Type::Method { binder, params, result, .. } => {
                let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
                Ok(Some(self.u.subst(result, binder, &names, arg_types)))
            }
            _ => Ok(None),
        }
    }

    /// A copy of alternative `alt` of type `info`, entered nowhere, its default arguments those
    /// of the clause of `alt`'s original after the skipped ones.
    fn mapped_alternative(&mut self, alt: SymbolId, info: TypeId) -> SymbolId {
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
        let (original, skipped) = self.mapped_alts.get(&alt).copied().unwrap_or((alt, 0));
        self.mapped_alts.insert(copy, (original, skipped + 1));
        copy
    }
}
