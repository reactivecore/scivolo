//! Named arguments and arguments left to defaults (dotty's `Application.reorder`,
//! `matchArgs`), where neither the function nor the arguments need lifting.

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::FxHashMap;
use syntax::ast::{NodeId, Tree};
use tasty::{SymbolId, TypeId};

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
    /// `Err` for named or missing arguments of a method with a `repeated` parameter or of
    /// `overloaded` alternatives.
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
        if repeated || overloaded || !named && args.len() > params.len() {
            return Err("named or default arguments of a repeated or overloaded method, or wrong arity");
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

    /// The arguments `aligned` of call `f` of `method` typed against `formals`, a missing one
    /// its default; `Err` where dotty would lift the function or the arguments.
    pub fn typed_aligned_args(
        &mut self,
        f: TNodeId,
        method: Option<SymbolId>,
        (args, aligned): (&[NodeId], &Aligned),
        formals: &[TypeId],
        ctx: Ctx,
    ) -> Result<Vec<TNodeId>, &'static str> {
        if aligned.contains(&None) && self.needs_lift_for_defaults(f) {
            return Err("default arguments needing lifting");
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
            return Err("reordered arguments needing lifting");
        }
        Ok(typed)
    }
}
