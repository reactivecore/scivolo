//! Named arguments and arguments left to defaults (dotty's `Application.reorder`,
//! `matchArgs`), where neither the function nor the arguments need lifting.

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use syntax::ast::{NodeId, Tree};
use tasty::{SymbolId, Type, TypeId};

/// The arguments of a call aligned with the parameters, `None` where one gets none.
pub type Aligned = Vec<Option<NodeId>>;

impl Typer<'_> {
    /// The name of argument `a` if it is named.
    fn arg_name(&self, a: NodeId) -> Option<String> {
        match *self.ast.tree(a) {
            Tree::NamedArg { name, .. } => Some(self.text(name.name).to_owned()),
            _ => None,
        }
    }

    /// `args` aligned with `params`, the non-repeated parameters, if some are named or parameters
    /// are left to defaults, `None` if they are positional and as many as the parameters take;
    /// `Err` for named or missing arguments of a method with a `repeated` parameter, and for named
    /// arguments of `overloaded` alternatives.
    pub fn aligned_args(
        &self,
        args: &[NodeId],
        params: &[(String, TypeId)],
        repeated: bool,
        overloaded: bool,
    ) -> Result<Option<Aligned>, &'static str> {
        let named = args.iter().any(|&a| self.arg_name(a).is_some());
        let fits = if repeated { args.len() >= params.len() } else { args.len() == params.len() };
        if !named && fits {
            return Ok(None);
        }
        if repeated || overloaded && named || !named && args.len() > params.len() {
            return Err(
                "named or default arguments of a repeated method, named ones of an overloaded one, or wrong arity",
            );
        }
        let names: Vec<String> = params.iter().map(|(n, _)| n.clone()).collect();
        self.ordered_args(args, &names).map(Some)
    }

    /// `args` aligned with the parameters `names` (dotty's `reorder`), as many as there are
    /// parameters.
    pub fn ordered_args(&self, args: &[NodeId], names: &[String]) -> Result<Aligned, &'static str> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < args.len() {
            match self.arg_name(args[i]) {
                Some(n) if names.get(out.len()) == Some(&n) => out.push(Some(args[i])),
                Some(_) => break,
                None => out.push(Some(args[i])),
            }
            i += 1;
        }
        if i < args.len() {
            let named: FxHashMap<String, NodeId> =
                args[i..].iter().filter_map(|&a| self.arg_name(a).map(|n| (n, a))).collect();
            self.handle_named(&names[out.len().min(names.len())..], &args[i..], named, &mut out)?;
        }
        if out.len() > names.len() {
            return Err("too many arguments");
        }
        out.resize(names.len(), None);
        Ok(out)
    }

    /// dotty's `handleNamed`: the arguments from the first named one on.
    fn handle_named(
        &self,
        mut pnames: &[String],
        mut args: &[NodeId],
        mut named: FxHashMap<String, NodeId>,
        out: &mut Aligned,
    ) -> Result<(), &'static str> {
        let mut dropped: Vec<String> = Vec::new();
        let mut missing = false;
        loop {
            if let Some(p) = pnames.first()
                && let Some(a) = named.remove(p)
            {
                out.push(Some(a));
                dropped.push(p.clone());
                pnames = &pnames[1..];
                continue;
            }
            let Some((&a, rest)) = args.split_first() else {
                if pnames.is_empty() {
                    return Ok(());
                }
                pnames = &pnames[1..];
                continue;
            };
            match self.arg_name(a) {
                Some(n) if dropped.contains(&n) => {
                    dropped.retain(|d| *d != n);
                    args = rest;
                }
                Some(n) if !pnames.is_empty() && named.contains_key(&n) => {
                    out.push(None);
                    pnames = &pnames[1..];
                    missing = true;
                }
                Some(_) => return Err("named argument without a parameter"),
                None if !dropped.is_empty() || missing => return Err("positional after named argument"),
                None => {
                    out.push(Some(a));
                    pnames = pnames.get(1..).unwrap_or(&[]);
                    args = rest;
                }
            }
        }
    }

    /// Call `f` of `method` and its arguments `aligned`, typed against `formals`, the types of
    /// `params`, a missing one its default, with the definitions lifted out of them (dotty's
    /// `TypedApply` with `liftFun` and `liftArgs`).
    pub fn typed_aligned_args(
        &mut self,
        (mut f, method): (TNodeId, Option<SymbolId>),
        (args, aligned): (&[NodeId], &Aligned),
        (params, formals): (&[(String, TypeId)], &[TypeId]),
        ctx: Ctx,
    ) -> Result<(TNodeId, Vec<TNodeId>, Vec<TNodeId>), &'static str> {
        let complex = method.is_some_and(|m| self.has_defaults(m));
        let tpe = self.tpd.tpe(f);
        let info = self.method_info(tpe).unwrap_or(tpe);
        let contextual = matches!(self.u.types().get(info), Type::Method { contextual: true, .. });
        let at = self.tpd.span(f).start_pos();
        let mut defs = Vec::new();
        let lifted_fun = aligned.contains(&None);
        if lifted_fun {
            f = self.lift_app(&mut defs, f, true, ctx)?;
        }
        let mut typed = Vec::new();
        let mut safe = true;
        for (i, slot) in aligned.iter().enumerate() {
            let formal = formals[i];
            let arg = match *slot {
                Some(a) => match *self.ast.tree(a) {
                    Tree::NamedArg { name, arg } => {
                        let arg = self.typed(arg, Pt::Type(formal), ctx);
                        let tpe = self.tpd.get(arg).tpe;
                        self.tpd.node(T::NamedArg { name, arg }, self.ast.span(a), tpe, None)
                    }
                    _ => self.typed(a, Pt::Type(formal), ctx),
                },
                None => {
                    let method = method.ok_or("default argument")?;
                    if contextual && !self.may_default(f, method, i) {
                        typed.push(self.implicit_arg(formal, at, ctx, 0)?);
                        continue;
                    }
                    let getter = self.default_arg(f, method, i, ctx)?;
                    typed.push(self.adapt(getter, Pt::Type(formal), ctx));
                    continue;
                }
            };
            safe &= self.is_pure_expr(arg);
            typed.push(arg);
        }
        let given = aligned.iter().rposition(Option::is_some).map_or(0, |i| i + 1);
        let in_order = given == args.len() && aligned[..given].iter().zip(args).all(|(a, b)| *a == Some(*b));
        if !in_order && !safe {
            if !lifted_fun {
                f = self.lift_app(&mut defs, f, complex, ctx)?;
            }
            let mut arg_defs = Vec::new();
            typed = self.lift_args(&mut arg_defs, params, &typed, complex, ctx);
            let position =
                |i: usize| aligned[i].and_then(|a| args.iter().position(|&b| b == a)).unwrap_or(args.len());
            arg_defs.sort_by_key(|&(i, _)| position(i));
            defs.extend(arg_defs.into_iter().map(|(_, d)| d));
        }
        let curried = self.result_of(f).is_some_and(|r| matches!(self.u.types().get(r), Type::Method { .. }));
        if !defs.is_empty() && curried {
            return Err("lifted call of a curried method");
        }
        Ok((f, typed, defs))
    }

    /// Whether parameter `n` of the clause call `f` of `method` applies may have a default
    /// argument.
    fn may_default(&mut self, f: TNodeId, method: SymbolId, n: usize) -> bool {
        match self.tpd.tree(f) {
            T::Apply { .. } => self.has_defaults(method),
            _ => self.default_params(method).get(n) == Some(&true),
        }
    }
}
