// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The implicit searches of `quotes.reflect`, which the typer answers between runs of the
//! expansion (`Implicits.search`), and `CompilationInfo`.

use super::Fallible;
use super::decode::symbol;
use crate::host::Host;
use tasty::tags::{GIVEN, IMPLICIT};
use tasty::{SymbolId, Type, TypeId};
use typer::inlining::{Annotations, Search};
use typer::tpd::{TNodeId, Tree as T};

/// What the typer gives an expansion: the unit's annotations, the implicit searches answered
/// so far and whether it expands while typing; the search the expansion asks for next.
#[derive(Default)]
pub struct FromTyper {
    pub annotations: Annotations,
    pub searched: Vec<(Search, Option<TNodeId>)>,
    pub while_typing: bool,
    pub wanted: Option<Search>,
}

/// The refusal that stops a run asking for a search.
pub const SEARCH: &str = "an implicit search the typer answers";

impl Host {
    pub fn is_while_typing(&self) -> bool {
        self.typer.while_typing
    }

    pub fn macro_settings(&self) -> String {
        self.settings.macro_settings.join("\n")
    }

    /// The implicit argument of type `pt` found as `[1, tree]`, `[0]` for none, the first `n` of
    /// `rest` the symbols of candidates left out (`Implicits.searchIgnoring`), the others the
    /// context of the `Quotes` searching: the quotes whose holes it fills, the symbols `-1 - s`
    /// whose `asQuotes` it is; refused where that context may define candidates.
    pub fn search(&mut self, pt: TypeId, n: i32, rest: Vec<i32>) -> Fallible<Vec<i32>> {
        let (ignored, context) = rest.split_at((n.max(0) as usize).min(rest.len()));
        let ignored = ignored.iter().map(|&s| symbol(s)).collect::<Fallible<Vec<_>>>()?;
        for &c in context {
            let local = match c {
                q if q >= 0 => self.defines_givens(self.tree(q as u32)?),
                s => self.has_local_givens(symbol(-1 - s)?),
            };
            if local {
                return Err("an implicit search inside a quote or owner that may define candidates");
            }
        }
        self.answered(Search { pt, ignored })
    }

    /// Whether quote `t` defines a given or an implicit, a parameter or class member too.
    fn defines_givens(&self, t: TNodeId) -> bool {
        self.tpd.fold(false, t, &mut |found, d| {
            found
                || matches!(self.tpd.tree(d), T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. })
                    && self.tpd.sym(d).is_some_and(|s| self.is_given(s))
        })
    }

    /// Whether `s` or an owner of it up to the splice owner may bring a given into scope: a
    /// given or a method with implicit parameters; any owner outside the splice owner.
    fn has_local_givens(&mut self, mut s: SymbolId) -> bool {
        while s != self.owner {
            let info = self.u.info(s);
            let implicit = matches!(
                *self.u.types().get(info),
                Type::Method { implicit: true, .. } | Type::Method { contextual: true, .. }
            ) || self.u.symbol(s).decls.iter().any(|&p| self.is_given(p));
            let Some(owner) = self.u.symbol(s).owner.filter(|_| !implicit && !self.is_given(s)) else {
                return true;
            };
            s = owner;
        }
        false
    }

    fn is_given(&self, s: SymbolId) -> bool {
        let flags = self.u.symbol(s).flags;
        flags.has(GIVEN) || flags.has(IMPLICIT)
    }

    fn answered(&mut self, search: Search) -> Fallible<Vec<i32>> {
        let answered = self.typer.searched.iter().find(|(s, _)| *s == search).map(|&(_, found)| found);
        match answered {
            Some(Some(t)) => Ok(vec![1, self.add_tree(t) as i32]),
            Some(None) => Ok(vec![0]),
            None => {
                self.typer.wanted.get_or_insert(search);
                Err(SEARCH)
            }
        }
    }
}
