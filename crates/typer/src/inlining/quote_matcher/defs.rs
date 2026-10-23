// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuoteMatcher.scala
//! Method definitions in quote patterns (dotty's `Match def` case of `treeMatch`).

use super::{Matched, Matcher};
use crate::tpd::{Clauses, TNodeId, Tree as T};
use tasty::SymbolId;

impl Matcher<'_> {
    /// `def` `s` of parameters `ps` and result `st`, `sr` against `p` of `pp`, `pt`, `pr`: the
    /// parameters clause by clause, each in the environment of those before it, then the result
    /// type and the right-hand side with the methods paired too.
    pub fn def_def(
        &mut self,
        (s, ps, st, sr): (TNodeId, Clauses, TNodeId, Option<TNodeId>),
        (p, pp, pt, pr): (TNodeId, Clauses, TNodeId, Option<TNodeId>),
    ) -> Matched {
        let (c1, c2) = (self.tpd.clause_items(ps).to_vec(), self.tpd.clause_items(pp).to_vec());
        if c1.len() != c2.len() {
            return Ok(false);
        }
        let mut pairs = Vec::new();
        for (a, b) in c1.into_iter().zip(c2) {
            let (a, b) = (self.tpd.items(a).to_vec(), self.tpd.items(b).to_vec());
            if a.len() != b.len() {
                return Ok(false);
            }
            for (x, y) in a.into_iter().zip(b) {
                if [x, y].iter().any(|&t| matches!(self.tpd.tree(t), T::TypeDef { .. })) {
                    return Err("a quote pattern with a polymorphic method definition");
                }
                if !self.with_pairs(&pairs, |m| m.matches(x, y))? {
                    return Ok(false);
                }
                pairs.extend(self.tpd.sym(x).zip(self.tpd.sym(y)));
            }
        }
        pairs.extend(self.tpd.sym(s).zip(self.tpd.sym(p)));
        self.with_pairs(&pairs, |m| {
            Ok(m.matches(st, pt)?
                && match (sr, pr) {
                    (Some(a), Some(b)) => m.matches(a, b)?,
                    (a, b) => a.is_none() && b.is_none(),
                })
        })
    }

    fn with_pairs(
        &mut self,
        pairs: &[(SymbolId, SymbolId)],
        body: impl FnOnce(&mut Self) -> Matched,
    ) -> Matched {
        let depth = self.env.len();
        self.env.extend_from_slice(pairs);
        let matched = body(self);
        self.env.truncate(depth);
        matched
    }
}
