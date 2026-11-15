// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Arguments aligned with the parameters by their names (dotty's `Application.reorder`).

use super::Typer;
use rustc_hash::FxHashMap;
use syntax::ast::{NodeId, Tree};
use tasty::{NameId, TypeId};

/// The arguments of a call aligned with the parameters, `None` where one gets none.
pub type Aligned = Vec<Option<NodeId>>;

impl Typer<'_> {
    /// The name of argument `a` if it is named.
    pub fn arg_name(&self, a: NodeId) -> Option<String> {
        match *self.ast.tree(a) {
            Tree::NamedArg { name, .. } => Some(self.text(name.name).to_owned()),
            _ => None,
        }
    }

    /// `args` aligned with `params`, the non-repeated parameters, if some are named or parameters
    /// are left to defaults, `None` if they are positional and as many as the parameters take;
    /// `Err` for named or missing arguments of a method with a `repeated` parameter.
    pub fn aligned_args(
        &self,
        args: &[NodeId],
        params: &[(NameId, TypeId)],
        repeated: bool,
    ) -> Result<Option<Aligned>, &'static str> {
        let named = args.iter().any(|&a| self.arg_name(a).is_some());
        let fits = if repeated { args.len() >= params.len() } else { args.len() == params.len() };
        if !named && fits {
            return Ok(None);
        }
        if repeated || !named && args.len() > params.len() {
            return Err("named or default arguments of a repeated method, or wrong arity");
        }
        let names: Vec<String> = params.iter().map(|&(n, _)| self.u.name(n).to_owned()).collect();
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
}
