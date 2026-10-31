// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The implicit searches of `quotes.reflect`, which the typer answers between runs of the
//! expansion (`Implicits.search`), and `CompilationInfo`.

use super::Fallible;
use super::decode::symbol;
use crate::host::Host;
use tasty::tags::{GIVEN, IMPLICIT};
use tasty::{SymbolId, TypeId};
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

    /// The implicit argument of type `pt` found as `[1, tree]`, `[0]` for none, candidates
    /// `ignored` left out (`Implicits.searchIgnoring`), searched by the `asQuotes` of symbol
    /// `owner` or its holes' (`-1` for none); refused where an owner or an enclosing quote may
    /// define candidates.
    pub fn search(&mut self, pt: TypeId, owner: i32, ignored: Vec<SymbolId>) -> Fallible<Vec<i32>> {
        let owned = owner >= 0 && self.has_local_givens(symbol(owner)?);
        if owned || self.open_quotes().into_iter().any(|q| self.defines_givens(q)) {
            return Err("an implicit search inside a quote or owner that may define candidates");
        }
        self.answered(Search { pt, ignored })
    }

    /// Whether quote `t` holds an import or defines a given or an implicit: a parameter, a
    /// pattern variable or a class with one as a member, inherited too.
    fn defines_givens(&mut self, t: TNodeId) -> bool {
        let mut imports = false;
        let defs = self.tpd.fold(Vec::new(), t, &mut |mut defs, d| {
            match self.tpd.tree(d) {
                T::Import { .. } => imports = true,
                T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } | T::Bind { .. } => {
                    defs.extend(self.tpd.sym(d))
                }
                _ => {}
            }
            defs
        });
        imports || defs.into_iter().any(|s| self.is_given(s) || self.class_givens(s))
    }

    /// Whether `s` or an owner of it up to the splice owner is a given or a class with a given
    /// member, or `s` is outside the splice owner.
    fn has_local_givens(&mut self, mut s: SymbolId) -> bool {
        while s != self.owner {
            let Some(owner) = self.u.symbol(s).owner.filter(|_| !self.is_given(s) && !self.class_givens(s))
            else {
                return true;
            };
            s = owner;
        }
        false
    }

    /// Whether `s` is a class with a given or an implicit member, inherited too.
    fn class_givens(&mut self, s: SymbolId) -> bool {
        self.u.is_class(s)
            && self
                .u
                .linearization(s)
                .into_iter()
                .any(|b| self.u.symbol(b).decls.iter().any(|&m| self.is_given(m)))
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
