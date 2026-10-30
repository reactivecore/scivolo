// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! `Select.overloaded` (dotty's `applyOverloaded`): the only member of the name, else the
//! alternative applicable to the arguments and more specific than the other applicable ones,
//! without implicit conversions.

use super::Fallible;
use crate::host::Host;
use tasty::{SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree as T};

impl Host {
    /// `Select.overloaded(qual, name, targs, args)`, `rest` the `n` type arguments and the
    /// arguments.
    pub fn select_overloaded(
        &mut self,
        qual: TNodeId,
        n: i32,
        rest: Vec<i32>,
        name: String,
    ) -> Fallible<TNodeId> {
        let (targs, terms) = rest.split_at((n as usize).min(rest.len()));
        let targs = targs.iter().map(|&t| self.type_of(t as u32)).collect::<Result<Vec<_>, _>>()?;
        let terms = terms.iter().map(|&t| self.tree(t as u32)).collect::<Result<Vec<_>, _>>()?;
        let tpe = self.tpd.tpe(qual);
        let mut members = self.u.member_of_type(tpe, &name);
        members.retain(|&m| self.u.symbol(m).kind.is_term());
        let chosen = match members[..] {
            [m] => m,
            _ => self.most_specific(qual, &members, &targs, &terms)?,
        };
        self.applied_with_defaults(qual, chosen, &targs, &terms)
    }

    /// The alternative of `members` applicable to `terms` and more specific than the others.
    fn most_specific(
        &mut self,
        qual: TNodeId,
        members: &[SymbolId],
        targs: &[TypeId],
        terms: &[TNodeId],
    ) -> Fallible<SymbolId> {
        if terms.iter().any(|&t| matches!(self.tpd.tree(t), T::NamedArg { .. })) {
            return Err("Select.overloaded of overloaded alternatives with named arguments");
        }
        let arg_types: Vec<TypeId> = terms.iter().map(|&t| self.tpd.tpe(t)).collect();
        let mut applicable = Vec::new();
        for &m in members {
            let Ok(fun) = self.selected(qual, m, targs) else { continue };
            if let Some(params) = self.params(fun)
                && params.len() == arg_types.len()
                && arg_types.iter().zip(&params).all(|(&a, &p)| self.u.is_subtype(a, p))
            {
                applicable.push((m, params));
            }
        }
        let specific: Vec<SymbolId> = (0..applicable.len())
            .filter(|&i| {
                let mine = applicable[i].1.clone();
                (0..applicable.len()).all(|j| {
                    i == j || mine.iter().zip(&applicable[j].1).all(|(&a, &b)| self.u.is_subtype(a, b))
                })
            })
            .map(|i| applicable[i].0)
            .collect();
        let [m] = specific[..] else {
            return Err("Select.overloaded without one most specific applicable alternative");
        };
        Ok(m)
    }

    /// `qual.m[targs]`.
    pub fn selected(&mut self, qual: TNodeId, m: SymbolId, targs: &[TypeId]) -> Fallible<TNodeId> {
        let sel = self.reflect().select(qual, m)?;
        match targs.is_empty() {
            true => Ok(sel),
            false => self.reflect().applied_to_types(sel, targs),
        }
    }

    /// The parameter types of the method type of `fun`, by-name ones as their results, if it has
    /// an explicit clause and no implicit one after it.
    fn params(&mut self, fun: TNodeId) -> Option<Vec<TypeId>> {
        let tpe = self.tpd.tpe(fun);
        let info = self.u.widen(tpe);
        let types = self.u.types();
        let Type::Method { params, result, implicit: false, .. } = types.get(info) else { return None };
        if matches!(types.get(*result), Type::Method { implicit: true, .. }) {
            return None;
        }
        let by_value = |p: TypeId| match *types.get(p) {
            Type::ByName(r) => r,
            _ => p,
        };
        Some(params.iter().map(|&(_, p)| by_value(p)).collect())
    }
}
