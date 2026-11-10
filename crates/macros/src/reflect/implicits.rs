// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The implicit searches of `quotes.reflect`, which the typer answers between runs of the
//! expansion (`Implicits.search`), and `CompilationInfo`.

use super::Fallible;
use super::decode::symbol;
use crate::host::Host;
use tasty::tags::{GIVEN, IMPLICIT};
use tasty::{Kind, SymbolId, Type, TypeId};
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

    /// The implicit argument of type `pt` found as `[1, tree]`, `[0]` for none, searched by the
    /// `asQuotes` of symbol `owner` or its holes' (`-1` for none). `rest` is `n`, then `n` pairs
    /// of a quote and its hole the searching `Quotes` fill, outermost first (`n` is `-1` for
    /// `Quotes` not known to fill any), then the candidates left out (`Implicits.searchIgnoring`).
    /// The givens and implicits of the owners and those quotes at those holes are in scope.
    pub fn search(&mut self, pt: TypeId, owner: i32, rest: Vec<i32>) -> Fallible<Vec<i32>> {
        const REFUSED: &str = "an implicit search inside a quote or owner that may define candidates";
        let (&n, rest) = rest.split_first().ok_or(super::decode::NO_ARGUMENTS)?;
        let (holes, ignored) =
            rest.split_at_checked(2 * n.max(0) as usize).ok_or(super::decode::NO_ARGUMENTS)?;
        let ignored = ignored.iter().map(|&s| symbol(s)).collect::<Fallible<Vec<_>>>()?;
        let holes = match n {
            0.. => holes.chunks(2).map(|p| self.hole_at(p[0], p[1])).collect::<Fallible<Vec<_>>>()?,
            _ => self.running_holes(),
        };
        let mut locals = match owner {
            0.. => self.owner_givens(symbol(owner)?)?.ok_or(REFUSED)?,
            _ => Vec::new(),
        };
        for (root, hole) in holes {
            let found = self.quote_givens(root, hole).ok_or(REFUSED)?;
            if n < 0 && !found.is_empty() {
                return Err(REFUSED);
            }
            locals.extend(found);
        }
        self.answered(Search { pt, ignored, locals })
    }

    /// The givens and implicits of quote `root` in scope at `hole`; none if it holds an import
    /// or a class inheriting one.
    fn quote_givens(&mut self, root: TNodeId, hole: TNodeId) -> Option<Vec<SymbolId>> {
        let (imports, types) =
            self.tpd.fold((false, Vec::new()), root, &mut |(mut imports, mut types), d| {
                match self.tpd.tree(d) {
                    T::Import { .. } => imports = true,
                    T::TypeDef { .. } => types.extend(self.tpd.sym(d)),
                    _ => {}
                }
                (imports, types)
            });
        if imports || types.into_iter().any(|s| self.inherited_givens(s)) {
            return None;
        }
        let mut found = Vec::new();
        typer::inlining::implicit_defs_at(&self.tpd, &self.u, (root, hole), &mut found);
        Some(found)
    }

    /// The givens and implicits of `s` and its owners up to the splice owner: the parameters of
    /// implicit and contextual clauses of methods, the members of classes; none if one of them
    /// is a given, a class inheriting one or `s` is outside the splice owner.
    fn owner_givens(&mut self, mut s: SymbolId) -> Fallible<Option<Vec<SymbolId>>> {
        let mut found = Vec::new();
        while s != self.owner {
            let Some(owner) =
                self.u.symbol(s).owner.filter(|_| !self.is_given(s) && !self.inherited_givens(s))
            else {
                return Ok(None);
            };
            if self.u.is_class(s) {
                let decls = self.u.symbol(s).decls.clone();
                found.extend(
                    decls.into_iter().filter(|&m| self.u.symbol(m).kind.is_term() && self.is_given(m)),
                );
            } else if self.u.symbol(s).kind == Kind::Def {
                found.extend(self.given_params(s)?);
            }
            s = owner;
        }
        Ok(Some(found))
    }

    /// The parameters of the implicit and contextual clauses of method `s`.
    fn given_params(&mut self, s: SymbolId) -> Fallible<Vec<SymbolId>> {
        let clauses = self.method_params(s)?;
        let mut t = self.u.info(s);
        let mut found = Vec::new();
        for clause in clauses {
            match self.u.ty(t).clone() {
                Type::Method { result, implicit, contextual, .. } => {
                    if implicit || contextual {
                        found.extend(clause.into_iter().filter_map(|p| self.tpd.sym(p)));
                    }
                    t = result;
                }
                Type::Poly { result, .. } => t = result,
                _ => break,
            }
        }
        Ok(found)
    }

    /// Whether `s` is a class with a base other than itself that has a given or an implicit
    /// member.
    fn inherited_givens(&mut self, s: SymbolId) -> bool {
        self.u.is_class(s)
            && self
                .u
                .linearization(s)
                .into_iter()
                .any(|b| b != s && self.u.symbol(b).decls.iter().any(|&m| self.is_given(m)))
    }

    fn is_given(&self, s: SymbolId) -> bool {
        let flags = self.u.symbol(s).flags;
        flags.has(GIVEN) || flags.has(IMPLICIT)
    }

    /// Whether `earlier`, a search of an earlier run, is `search`, whose local candidates are
    /// those of this run in the same order.
    fn same_search(&self, earlier: &Search, search: &Search) -> bool {
        let names = |s: &Search| s.locals.iter().map(|&l| self.u.symbol(l).name.clone()).collect::<Vec<_>>();
        earlier.pt == search.pt && earlier.ignored == search.ignored && names(earlier) == names(search)
    }

    /// Argument `found` of an earlier run's search with local candidates `earlier`, referring
    /// to `now`'s instead, at its span.
    fn relocated(&mut self, found: TNodeId, earlier: &[SymbolId], now: &[SymbolId]) -> Fallible<TNodeId> {
        if let Some(i) = self.tpd.sym(found).and_then(|s| earlier.iter().position(|&l| l == s))
            && matches!(self.tpd.tree(found), T::Ident { .. } | T::Select { .. })
        {
            let span = self.tpd.span(found);
            let t = match *self.tpd.tree(found) {
                T::Ident { name } => {
                    let tpe = self.reflect().symbol_ref(now[i]);
                    self.tpd.node(T::Ident { name }, span, Some(tpe), Some(now[i]))
                }
                _ => self.reflect().ref_(now[i])?,
            };
            self.tpd.get_mut(t).span = span;
            return Ok(t);
        }
        let stale = self.tpd.fold(false, found, &mut |stale, t| {
            stale || self.tpd.sym(t).is_some_and(|s| earlier.contains(&s))
        });
        match stale {
            true => Err("an implicit argument applying a given of the quote"),
            false => Ok(found),
        }
    }

    fn answered(&mut self, search: Search) -> Fallible<Vec<i32>> {
        let at = self.typer.searched.iter().position(|(s, _)| self.same_search(s, &search));
        let answered = match at.map(|k| self.typer.searched[k].clone()) {
            Some((earlier, Some(t))) => {
                let t = self.relocated(t, &earlier.locals, &search.locals)?;
                self.typer.searched[at.expect("found")].1 = Some(t);
                Some(Some(t))
            }
            Some((_, None)) => Some(None),
            None => None,
        };
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
