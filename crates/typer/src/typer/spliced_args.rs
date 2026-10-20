// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The arguments of earlier clauses a default getter of a later clause is applied to, copied
//! (dotty's `spliceMeth`).

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::Type;

impl Typer<'_> {
    /// Copies of arguments `args` of a call of `fun` for the default getter of a later clause: a
    /// by-name argument other than a path with fresh symbols for its definitions (dotty's
    /// `duplicateByNameArg`).
    pub fn spliced_args(&mut self, fun: TNodeId, args: &[TNodeId]) -> Result<Vec<TNodeId>, &'static str> {
        let t = self.tpd.tpe(fun);
        let info = self.method_info(t).unwrap_or(t);
        let params = match self.u.types().get(info).clone() {
            Type::Method { params, .. } => params,
            _ => Vec::new(),
        };
        let mut out = Vec::new();
        for (i, &a) in args.iter().enumerate() {
            let by_name =
                params.get(i).is_some_and(|&(_, p)| matches!(self.u.types().get(p), Type::ByName(_)));
            out.push(match self.is_copyable(a) {
                true => self.copied_path(a),
                false if by_name => self
                    .with_inlining(false, |i| i.fresh_copy(a))
                    .map_err(|_| "default argument after a by-name argument outside the subset")?,
                false => return Err("default argument after an argument list"),
            });
        }
        Ok(out)
    }

    /// Whether `t` is a path, a literal or a call of paths, which `copied_path` copies whole.
    fn is_copyable(&self, t: TNodeId) -> bool {
        match *self.tpd.tree(t) {
            T::Ident { .. } | T::Literal { .. } | T::This { .. } => true,
            T::Select { qualifier, .. } => self.is_copyable(qualifier),
            T::TypeApply { fun, .. } | T::Typed { expr: fun, .. } => self.is_copyable(fun),
            T::Apply { fun, args } => {
                self.is_copyable(fun) && self.tpd.items(args).iter().all(|&a| self.is_copyable(a))
            }
            T::SeqLiteral { elems, .. } => self.tpd.items(elems).iter().all(|&a| self.is_copyable(a)),
            _ => false,
        }
    }

    /// A copy of path or call `t`, its qualifiers, functions and arguments copied too.
    pub fn copied_path(&mut self, t: TNodeId) -> TNodeId {
        let tree = match self.tpd.tree(t).clone() {
            T::Select { qualifier, name } => T::Select { qualifier: self.copied_path(qualifier), name },
            T::This { qual } => T::This { qual: self.copied_path(qual) },
            T::Typed { expr, tpt } => T::Typed { expr: self.copied_path(expr), tpt: self.copied_path(tpt) },
            T::SeqLiteral { elems, elemtpt } => {
                let items: Vec<TNodeId> =
                    self.tpd.items(elems).to_vec().into_iter().map(|a| self.copied_path(a)).collect();
                T::SeqLiteral { elems: self.tpd.list(&items), elemtpt: self.copied_path(elemtpt) }
            }
            T::Apply { fun, args } | T::TypeApply { fun, args } => {
                let fun = self.copied_path(fun);
                let items: Vec<TNodeId> =
                    self.tpd.items(args).to_vec().into_iter().map(|a| self.copied_path(a)).collect();
                let args = self.tpd.list(&items);
                match self.tpd.tree(t) {
                    T::Apply { .. } => T::Apply { fun, args },
                    _ => T::TypeApply { fun, args },
                }
            }
            tree => tree,
        };
        let (span, tpe, sym) = (self.tpd.span(t), self.tpd.get(t).tpe, self.tpd.sym(t));
        self.tpd.node(tree, span, tpe, sym)
    }
}
