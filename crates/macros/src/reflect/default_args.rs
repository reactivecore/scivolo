// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/EtaExpansion.scala
//! The application `Select.overloaded` builds (dotty's `ApplyTo`): named arguments put in
//! parameter order, missing ones the default getters' results, the receiver and impure
//! arguments lifted into values where the order changes or defaults are read.

use super::Fallible;
use crate::host::Host;
use std::collections::HashMap;
use tasty::tags::{LAZY, MUTABLE};
use tasty::{Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree as T};

impl Host {
    /// `qual.m[targs](terms)`.
    pub fn applied_with_defaults(
        &mut self,
        qual: TNodeId,
        m: SymbolId,
        targs: &[TypeId],
        terms: &[TNodeId],
    ) -> Fallible<TNodeId> {
        let fun = self.selected(qual, m, targs)?;
        let info = self.u.widen(self.tpd.tpe(fun));
        let Type::Method { params, .. } = self.u.types().get(info) else {
            return self.reflect().apply(fun, terms);
        };
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let named = |h: &Host, t: TNodeId| matches!(h.tpd.tree(t), T::NamedArg { .. });
        if terms.len() == names.len() && !terms.iter().any(|&t| named(self, t)) {
            return self.reflect().apply(fun, terms);
        }
        let ordered = self.ordered(&names, terms)?;
        let reordered =
            ordered.len() != terms.len() || ordered.iter().zip(terms).any(|(o, &t)| *o != Some(t));
        let lift_args = reordered && !terms.iter().all(|&t| self.is_safe(self.unnamed(t)));
        let (mut qual, mut defs, mut lifted) = (qual, Vec::new(), HashMap::new());
        if (lift_args || ordered.contains(&None)) && !self.is_safe(qual) {
            qual = self.lifted(qual, &mut defs)?;
        }
        if lift_args {
            let same =
                terms.iter().rev().zip(ordered.iter().rev()).take_while(|&(&t, o)| *o == Some(t)).count();
            for &t in &terms[..terms.len() - same] {
                let arg = self.unnamed(t);
                if !self.is_safe(arg) {
                    lifted.insert(t, self.lifted(arg, &mut defs)?);
                }
            }
        }
        let fun = self.selected(qual, m, targs)?;
        let mut args = Vec::new();
        for (i, o) in ordered.into_iter().enumerate() {
            args.push(match o {
                Some(t) => lifted.get(&t).copied().unwrap_or_else(|| self.unnamed(t)),
                None => self.default_arg(qual, m, i, targs)?,
            });
        }
        let app = self.reflect().apply(fun, &args)?;
        match defs.is_empty() {
            true => Ok(app),
            false => self.reflect().block(&defs, app),
        }
    }

    /// `terms` by parameter `names`, `None` where a parameter has no argument.
    fn ordered(&mut self, names: &[String], terms: &[TNodeId]) -> Fallible<Vec<Option<TNodeId>>> {
        let mut ordered = vec![None; names.len()];
        let mut seen_named = false;
        for (i, &t) in terms.iter().enumerate() {
            let at = match *self.tpd.tree(t) {
                T::NamedArg { name, .. } => {
                    seen_named = true;
                    let text = self.names.text(name.name);
                    names
                        .iter()
                        .position(|n| n == text)
                        .ok_or("Select.overloaded with an unknown argument name")?
                }
                _ if seen_named => {
                    return Err("Select.overloaded with a positional argument after a named one");
                }
                _ => i,
            };
            let slot = ordered.get_mut(at).ok_or("Select.overloaded with more arguments than parameters")?;
            if slot.replace(t).is_some() {
                return Err("Select.overloaded with an argument given twice");
            }
        }
        Ok(ordered)
    }

    /// The default argument of parameter `i` of method `m` of `qual`, `qual.m$default$N[targs]`.
    fn default_arg(&mut self, qual: TNodeId, m: SymbolId, i: usize, targs: &[TypeId]) -> Fallible<TNodeId> {
        let name = format!("{}$default${}", self.u.symbol(m).name, i + 1);
        let tpe = self.tpd.tpe(qual);
        let getter = self.u.member_of_type(tpe, &name).first().copied();
        let getter = getter.ok_or("Select.overloaded without an argument of a parameter without default")?;
        let sel = self.selected(qual, getter, targs)?;
        let info = self.u.widen(self.tpd.tpe(sel));
        match self.u.types().get(info) {
            Type::Poly { .. } | Type::Method { .. } => {
                Err("Select.overloaded of a default getter with parameters")
            }
            _ => Ok(sel),
        }
    }

    /// A reference to a new synthetic value of `rhs`, its definition added to `defs`.
    fn lifted(&mut self, rhs: TNodeId, defs: &mut Vec<TNodeId>) -> Fallible<TNodeId> {
        let name = self.fresh.next(&mut self.names, "x$");
        let name = self.names.text(name).to_owned();
        let owner = self.owner;
        let (def, r) = self.reflect().let_(owner, &name, rhs, 0)?;
        defs.push(def);
        Ok(r)
    }

    fn unnamed(&self, t: TNodeId) -> TNodeId {
        match *self.tpd.tree(t) {
            T::NamedArg { arg, .. } => arg,
            _ => t,
        }
    }

    /// Whether `t` is a literal, `this`, or a path of immutable eager values and objects.
    fn is_safe(&self, t: TNodeId) -> bool {
        let stable = |s: Option<SymbolId>| {
            s.is_some_and(|s| {
                let s = self.u.symbol(s);
                matches!(s.kind, Kind::Val | Kind::Package) && !s.flags.has(MUTABLE) && !s.flags.has(LAZY)
            })
        };
        match *self.tpd.tree(t) {
            T::Literal { .. } | T::This { .. } => true,
            T::Ident { .. } => stable(self.tpd.sym(t)),
            T::Select { qualifier, .. } => stable(self.tpd.sym(t)) && self.is_safe(qualifier),
            _ => false,
        }
    }
}
