// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/EtaExpansion.scala
//! The application `Select.overloaded` builds (dotty's `ApplyTo`): named arguments put in
//! parameter order, missing ones the default getters' results, the receiver and arguments
//! lifted into values as dotty's `liftFun` and `liftArgs` do.

use super::Fallible;
use crate::host::Host;
use tasty::{SymbolId, Type, TypeId};
use typer::inlining::Purity;
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
        let Type::Method { params, .. } = self.u.types().get(info).clone() else {
            return self.reflect().apply(fun, terms);
        };
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        let named = |h: &Host, t: TNodeId| matches!(h.tpd.tree(t), T::NamedArg { .. });
        if terms.len() == names.len() && !terms.iter().any(|&t| named(self, t)) {
            return self.reflect().apply(fun, terms);
        }
        let ordered = self.ordered(&names, terms)?;
        let complex = self.has_defaults(qual, m, info);
        let (mut qual, mut defs) = (qual, Vec::new());
        let lifted_fun = ordered.contains(&None);
        if lifted_fun {
            qual = self.lifted_prefix(qual, &mut defs, complex)?;
        }
        let (mut typed, mut safe) = (Vec::new(), true);
        for (i, o) in ordered.iter().enumerate() {
            typed.push(match *o {
                Some(t) => {
                    safe &= self.pure(|p| p.is_pure_expr(t));
                    t
                }
                None => self.default_arg(qual, m, i, targs)?,
            });
        }
        let given = ordered.iter().rposition(Option::is_some).map_or(0, |i| i + 1);
        let in_order =
            given == terms.len() && ordered[..given].iter().zip(terms).all(|(o, &t)| *o == Some(t));
        if !in_order && !safe {
            if !lifted_fun {
                qual = self.lifted_prefix(qual, &mut defs, complex)?;
            }
            let mut arg_defs = Vec::new();
            for (i, (arg, (name, p))) in typed.iter_mut().zip(&params).enumerate() {
                if !self.is_lifted_param(*p) {
                    continue;
                }
                let prefix = if name.contains('$') { "" } else { name.as_str() };
                let mut own = Vec::new();
                *arg = self.lifted_arg(*arg, prefix, complex, &mut own)?;
                arg_defs.extend(own.into_iter().map(|d| (i, d)));
            }
            let position = |i: usize| ordered[i].and_then(|a| terms.iter().position(|&t| t == a));
            arg_defs.sort_by_key(|&(i, _)| position(i).unwrap_or(terms.len()));
            defs.extend(arg_defs.into_iter().map(|(_, d)| d));
        }
        let fun = self.selected(qual, m, targs)?;
        let app = self.reflect().apply(fun, &typed)?;
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

    /// Whether method `m` of `qual`, of method type `info`, has a default getter (dotty's
    /// `hasDefaultParams`).
    fn has_defaults(&mut self, qual: TNodeId, m: SymbolId, info: TypeId) -> bool {
        let count: usize = self.u.param_clauses(info).iter().map(|(_, ps)| ps.len()).sum();
        let (tpe, name) = (self.tpd.tpe(qual), self.u.symbol(m).name.clone());
        (1..=count).any(|i| !self.u.member_of_type(tpe, &format!("{name}$default${i}")).is_empty())
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

    /// Receiver `qual` of a method with default arguments if `defaults`, kept if a literal,
    /// `this`, `new` or an idempotent reference, else lifted; of another method kept if
    /// idempotent (dotty's `liftApp` of a `Select`).
    fn lifted_prefix(&mut self, qual: TNodeId, defs: &mut Vec<TNodeId>, defaults: bool) -> Fallible<TNodeId> {
        let idempotent = |h: &mut Host| h.pure(|p| p.is_idempotent_expr(qual));
        let keep = match *self.tpd.tree(qual) {
            T::Literal { .. } | T::This { .. } | T::New { .. } => true,
            T::Ident { .. } | T::Select { .. } => idempotent(self),
            _ => !defaults && idempotent(self),
        };
        if keep { Ok(qual) } else { self.lifted(qual, "", defaults, defs) }
    }

    /// Argument `arg` lifted, inside its `NamedArg` (dotty's `liftArg`).
    fn lifted_arg(
        &mut self,
        arg: TNodeId,
        prefix: &str,
        complex: bool,
        defs: &mut Vec<TNodeId>,
    ) -> Fallible<TNodeId> {
        let T::NamedArg { name, arg: inner } = *self.tpd.tree(arg) else {
            return self.lifted(arg, prefix, complex, defs);
        };
        let lifted = self.lifted(inner, prefix, complex, defs)?;
        if lifted == inner {
            return Ok(arg);
        }
        let (span, tpe) = (self.tpd.span(arg), self.tpd.get(lifted).tpe);
        Ok(self.tpd.node(T::NamedArg { name, arg: lifted }, span, tpe, None))
    }

    /// `rhs`, or a reference to `val prefix$N = rhs` added to `defs`, `$N$` for no prefix, unless
    /// a pure path if `complex`, else pure (dotty's `lift` of `LiftComplex` and `LiftImpure`).
    fn lifted(
        &mut self,
        rhs: TNodeId,
        prefix: &str,
        complex: bool,
        defs: &mut Vec<TNodeId>,
    ) -> Fallible<TNodeId> {
        if self.pure(|p| if complex { p.is_pure_path(rhs) } else { p.is_pure_expr(rhs) }) {
            return Ok(rhs);
        }
        let name = match prefix {
            "" => self.fresh.unique(&mut self.names),
            p => self.fresh.next(&mut self.names, &format!("{p}$")),
        };
        let name = self.names.text(name).to_owned();
        let owner = self.owner;
        let (def, r) = self.reflect().let_(owner, &name, rhs, 0)?;
        defs.push(def);
        Ok(r)
    }

    /// Whether the argument of a parameter of type `p` may be lifted: not by-name, not
    /// `@InlineParam`.
    fn is_lifted_param(&mut self, p: TypeId) -> bool {
        match *self.u.types().get(p) {
            Type::ByName(_) => false,
            Type::Annotated(_, annot) => self
                .u
                .class_symbol(annot)
                .is_none_or(|c| self.u.full_name(c) != "scala.annotation.internal.InlineParam"),
            _ => true,
        }
    }

    fn pure<R>(&mut self, f: impl FnOnce(&mut Purity<'_>) -> R) -> R {
        let none = Default::default();
        f(&mut Purity { u: &mut self.u, tpd: &self.tpd, with_inits: &none })
    }
}
