//! The inferred type arguments dotty's typer gives the calls of a capture-checked file, with the
//! capture sets of the arguments that tell them.

use super::TreePickler;
use super::captured_exprs::Captured;
use super::inferred_captures::emptied;
use crate::tpd::{List, TNodeId, Tree as T};
use crate::typer::seen_from::{resolve_class_params, resolve_class_params_keeping_captures};
use crate::typer::type_apply::instantiate_poly;
use tasty::{Binder, NameId, SymbolId, Type, TypeId};

impl TreePickler<'_> {
    /// Applies type `info` of polymorphic method `method` to type arguments `targs`, each inferred
    /// one told by the arguments `clauses` give parameters of its type and recorded with its
    /// capture sets emptied; `None` if one is not told.
    pub fn instantiated(
        &mut self,
        (info, method): (TypeId, SymbolId),
        targs: List,
        clauses: &[List],
        depth: usize,
    ) -> Option<TypeId> {
        let poly = self.u.poly(info)?;
        let (binder, names, result) = (poly.binder, poly.names(), poly.result);
        let params: Vec<TypeId> =
            (0..names.len()).map(|i| self.u.intern_type(Type::Param(binder, i as u32))).collect();
        let result = instantiate_poly(self.u, result, (binder, &names), &params, Some(method));
        let targs = self.tpd.items(targs).to_vec();
        if targs.len() != names.len() {
            return None;
        }
        let mut args = Vec::new();
        for (i, &targ) in targs.iter().enumerate() {
            let tpe = self.tpd.get(targ).tpe?;
            if !matches!(self.tpd.tree(targ), T::TypeTree { inferred: true }) {
                args.push(tpe);
                continue;
            }
            match self.told_type_arg((binder, i, names[i]), result, clauses, depth)? {
                Captured::Ours => {
                    self.inferred.capture_free.insert(targ);
                    args.push(tpe);
                }
                Captured::With(t) => {
                    let kept = emptied(self.u, t);
                    let kept = resolve_class_params_keeping_captures(self.u, kept);
                    if resolve_class_params(self.u, kept) != tpe {
                        return None;
                    }
                    self.kept_captures.insert(targ, kept);
                    args.push(t);
                }
            }
        }
        Some(self.u.subst(result, binder, &names, &args))
    }

    /// The type the arguments of the parameters of type `param`, the `i`th type parameter `name`
    /// of binder `binder`, or a repetition of it, give it: the one all give.
    fn told_type_arg(
        &mut self,
        (binder, i, name): (Binder, usize, NameId),
        mut info: TypeId,
        clauses: &[List],
        depth: usize,
    ) -> Option<Captured> {
        let is_param = |u: &tasty::Universe, t: TypeId| match *u.ty(t) {
            Type::Param(b, k) => b == binder && k as usize == i,
            Type::Local(b, n) => b.0 == binder.0 && n == name,
            _ => false,
        };
        let (mut args, mut seq_tpts) = (Vec::new(), Vec::new());
        for &clause in clauses {
            let formals = self.u.method(info)?.param_types();
            info = self.u.method(info)?.result;
            let actuals = self.tpd.items(clause).to_vec();
            for (j, &formal) in formals.iter().enumerate() {
                if is_param(self.u, formal) {
                    args.push(*actuals.get(j)?);
                } else if self.u.repeated_elem(formal).is_some_and(|e| is_param(self.u, e)) {
                    args.extend(self.repeated_args(actuals.get(j..)?, &mut seq_tpts)?);
                }
            }
        }
        let mut found = None;
        for a in args {
            let c = self.captured_in(a, depth)?;
            if found.is_some_and(|f| f != c) {
                return None;
            }
            found = Some(c);
        }
        for tpt in seq_tpts {
            self.keep_element_captures(tpt, found?)?;
        }
        found
    }

    /// The elements of the repeated arguments `args`, a sequence literal's those of its own, whose
    /// element type tree and that of the ascription around it go to `seq_tpts`.
    fn repeated_args(&self, args: &[TNodeId], seq_tpts: &mut Vec<(TNodeId, bool)>) -> Option<Vec<TNodeId>> {
        let [one] = args else { return Some(args.to_vec()) };
        let (seq, tpt) = match *self.tpd.tree(*one) {
            T::Typed { expr, tpt } => (expr, Some(tpt)),
            _ => (*one, None),
        };
        match *self.tpd.tree(seq) {
            T::SeqLiteral { elems, elemtpt } => {
                seq_tpts.extend([(elemtpt, false)].into_iter().chain(tpt.map(|t| (t, true))));
                Some(self.tpd.items(elems).to_vec())
            }
            _ if tpt.is_some() => None,
            _ => Some(args.to_vec()),
        }
    }

    /// Records the type of inferred type tree `tpt` of a sequence literal's elements, or if
    /// `repeated` of the repeated type of their ascription, of elements of type `elem`, with its
    /// capture sets emptied.
    fn keep_element_captures(&mut self, (tpt, repeated): (TNodeId, bool), elem: Captured) -> Option<()> {
        if !matches!(self.tpd.tree(tpt), T::TypeTree { inferred: true }) {
            return Some(());
        }
        let Captured::With(elem) = elem else {
            self.inferred.capture_free.insert(tpt);
            return Some(());
        };
        let tpe = self.tpd.get(tpt).tpe?;
        let mut kept = emptied(self.u, elem);
        if repeated {
            let (tycon, _) = self.u.applied(tpe)?;
            kept = self.u.intern_applied(tycon, &[kept]);
        }
        let kept = resolve_class_params_keeping_captures(self.u, kept);
        if resolve_class_params(self.u, kept) != tpe {
            return None;
        }
        self.kept_captures.insert(tpt, kept);
        Some(())
    }
}
