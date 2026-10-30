// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuoteMatcher.scala
//! The runtime of `QuoteMatching.ExprMatch.unapply` (dotty's `QuoteMatcher.treeMatch`): a
//! scrutinee tree matched against a quote pattern's tree with typed term holes.

mod defs;
mod open;
mod open_calls;
mod open_closure;
mod shapes;

use super::Reflect;
use super::quote_match::Bindings;
use crate::tpd::{List, TNodeId, Tpd, Tree as T};
use crate::unpickle::{PatternBody, QuotePattern, Refusal};
use shapes::same_constant;
use syntax::names::Names;
use tasty::{SymbolId, TypeId, Universe};

/// What `scrutinee` gives the type bindings and term holes of `pattern` if it matches it: the
/// bindings' types and the holes' trees, each in the pattern's order, a higher-order hole's a
/// closure owned by `owner`. `fills` are the quote hole placeholders with the trees that fill
/// them, if filled.
pub fn quote_match(
    (u, tpd, names): (&mut Universe, &mut Tpd, &mut Names),
    (scrutinee, fills): (TNodeId, &[(TNodeId, Option<TNodeId>)]),
    pattern: &QuotePattern,
    owner: SymbolId,
) -> Result<Option<Matches>, Refusal> {
    let PatternBody::Term(tree) = pattern.body else {
        return Err("a type pattern matched against an expression");
    };
    let Some(bindings) = Bindings::new(u, &pattern.bindings, &pattern.spliced) else { return Ok(None) };
    let mut m = Matcher { u, tpd, names, fills, bindings, env: Vec::new(), matched: Vec::new() };
    if !m.matches(scrutinee, tree)? {
        return Ok(None);
    }
    let (types, holes) = (m.bindings.approximated(), m.matched);
    let span = tpd.span(scrutinee);
    let mut r = Reflect { u, tpd, names, span };
    let exprs = holes.into_iter().map(|h| match h {
        Hole::Closed(t) => Ok(t),
        Hole::Open(t, params, tparams) => r.open_closure(owner, t, (&params, &tparams)),
    });
    Ok(Some((types, exprs.collect::<Result<_, _>>()?)))
}

/// The types of a pattern's bindings and the trees its holes match.
pub type Matches = (Vec<TypeId>, Vec<TNodeId>);

type Matched = Result<bool, Refusal>;

/// What a term hole matched: a closed tree, or an open one with the higher-order hole's
/// captured identifiers and type parameters.
enum Hole {
    Closed(TNodeId),
    Open(TNodeId, Vec<Captured>, Vec<SymbolId>),
}

/// An identifier a higher-order hole captures: its name, the scrutinee's symbol it stands for
/// and, for a method, the type of its eta expansion.
pub struct Captured {
    pub name: String,
    pub sym: SymbolId,
    pub eta: Option<TypeId>,
}

struct Matcher<'a> {
    u: &'a mut Universe,
    tpd: &'a mut Tpd,
    names: &'a mut Names,
    fills: &'a [(TNodeId, Option<TNodeId>)],
    bindings: Bindings,
    /// The symbols the scrutinee defines with those of the pattern they stand for.
    env: Vec<(SymbolId, SymbolId)>,
    matched: Vec<Hole>,
}

impl Matcher<'_> {
    /// `scrutinee =?= pattern`.
    fn matches(&mut self, s: TNodeId, p: TNodeId) -> Matched {
        let (s, p) = (self.normalize(s), self.normalize(p));
        if let Some(&(_, filled)) = self.fills.iter().find(|&&(h, _)| h == s) {
            let filled = filled.ok_or("a quote pattern matched against a quote with an open hole")?;
            return self.matches(filled, p);
        }
        let (st, pt) = (self.tpd.tree(s).clone(), self.tpd.tree(p).clone());
        if let T::Typed { expr, tpt } = pt
            && let Some(hole) = self.hole(expr)
            && self.is_repeated(tpt)
        {
            return Ok(match st {
                T::Typed { expr: inner, .. } if self.conforms(inner, hole) => self.closed(s),
                _ => false,
            });
        }
        if let Some(hole) = self.hole(p) {
            return Ok(self.conforms(s, hole) && self.closed(s));
        }
        if let T::Apply { fun, args } = pt
            && let T::TypeApply { fun, args: targs } = *self.tpd.tree(fun)
            && (self.is_patterns(fun, "higherOrderHole") || self.is_patterns(fun, "higherOrderHoleWithTypes"))
        {
            return self.open(s, targs, args);
        }
        if let T::Typed { expr, .. } = pt {
            return self.matches(s, expr);
        }
        match (st, pt) {
            (T::Typed { expr, .. }, _) => self.matches(expr, p),
            (T::Literal { value: a }, T::Literal { value: b }) => Ok(same_constant(&a, &b)),
            (T::ClassLiteral { tpe: a }, T::ClassLiteral { tpe: b }) => Ok(self.same_type(a, b)),
            (T::Ident { .. } | T::Select { .. }, _) => self.reference(s, p),
            (T::Apply { fun: f1, args: a1 }, T::Apply { fun: f2, args: a2 })
            | (T::TypeApply { fun: f1, args: a1 }, T::TypeApply { fun: f2, args: a2 }) => {
                Ok(self.matches(f1, f2)? && self.lists(a1, a2)?)
            }
            (T::Block { stats: s1, expr: e1 }, T::Block { stats: s2, expr: e2 }) => {
                self.block((s, s1, e1), (p, s2, e2))
            }
            (T::If { cond: c1, thenp: t1, elsep: e1, .. }, T::If { cond: c2, thenp: t2, elsep: e2, .. }) => {
                Ok(self.matches(c1, c2)? && self.matches(t1, t2)? && self.matches(e1, e2)?)
            }
            (T::WhileDo { cond: c1, body: b1 }, T::WhileDo { cond: c2, body: b2 }) => {
                Ok(self.matches(c1, c2)? && self.matches(b1, b2)?)
            }
            (T::Assign { lhs: l1, rhs: r1 }, T::Assign { lhs: l2, rhs: r2 }) => {
                Ok(self.matches(l1, l2)? && self.matches(r1, r2)?)
            }
            (T::New { tpt: a }, T::New { tpt: b }) => {
                let (a, b) = (self.class_of(a), self.class_of(b));
                Ok(a.is_some() && a == b)
            }
            (T::This { .. }, T::This { .. }) => {
                let (a, b) = (self.this_class(s), self.this_class(p));
                Ok(a.is_some() && a == b)
            }
            (T::Super { .. }, T::Super { .. }) => Err("a quote pattern matched against a super reference"),
            (T::SeqLiteral { elems: a, .. }, T::SeqLiteral { elems: b, .. }) => self.lists(a, b),
            (T::ValDef { tpt: t1, rhs: r1, .. }, T::ValDef { tpt: t2, rhs: r2, .. }) => {
                if !self.same_val_flags(s, p) || !self.matches(t1, t2)? {
                    return Ok(false);
                }
                let pair = self.tpd.sym(s).zip(self.tpd.sym(p));
                self.with_env(pair, |m| match (r1, r2) {
                    (Some(r1), Some(r2)) => m.matches(r1, r2),
                    (r1, r2) => Ok(r1.is_none() && r2.is_none()),
                })
            }
            (
                T::DefDef { paramss: p1, tpt: t1, rhs: r1, .. },
                T::DefDef { paramss: p2, tpt: t2, rhs: r2, .. },
            ) => self.def_def((s, p1, t1, r1), (p, p2, t2, r2)),
            (T::Closure { .. }, T::Closure { .. }) => Ok(true),
            (T::NamedArg { name: n1, arg: a1 }, T::NamedArg { name: n2, arg: a2 }) => {
                Ok(n1.name == n2.name && self.matches(a1, a2)?)
            }
            _ if self.is_type_tree(s) => Ok(self.is_type_tree(p) && self.conforms(s, p)),
            _ => Ok(false),
        }
    }

    /// `t` without empty blocks, blocks nested in blocks but for closures, and inlined trees
    /// without bindings.
    fn normalize(&mut self, t: TNodeId) -> TNodeId {
        match *self.tpd.tree(t) {
            T::Block { stats, expr } if stats.is_empty() => self.normalize(expr),
            T::Block { stats, expr } => match *self.tpd.tree(expr) {
                T::Block { stats: inner, expr: e } if !matches!(self.tpd.tree(e), T::Closure { .. }) => {
                    let all = [self.tpd.items(stats), self.tpd.items(inner)].concat();
                    let b = self.block_of(&all, e, t);
                    self.normalize(b)
                }
                _ => t,
            },
            T::Inlined { bindings, expansion, .. } if bindings.is_empty() => self.normalize(expansion),
            _ => t,
        }
    }

    /// A block of `stats` and `expr` at the span of `at`.
    fn block_of(&mut self, stats: &[TNodeId], expr: TNodeId, at: TNodeId) -> TNodeId {
        let (span, tpe) = (self.tpd.span(at), self.tpd.get(expr).tpe);
        let stats = self.tpd.list(stats);
        self.tpd.node(T::Block { stats, expr }, span, tpe, None)
    }

    fn lists(&mut self, a: List, b: List) -> Matched {
        let (a, b) = (self.tpd.items(a).to_vec(), self.tpd.items(b).to_vec());
        if a.len() != b.len() {
            return Ok(false);
        }
        for (s, p) in a.into_iter().zip(b) {
            if !self.matches(s, p)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Blocks: their first statements, then blocks of the rest, the definitions' symbols paired.
    fn block(
        &mut self,
        (s, s1, e1): (TNodeId, List, TNodeId),
        (p, s2, e2): (TNodeId, List, TNodeId),
    ) -> Matched {
        let (s1, s2) = (self.tpd.items(s1).to_vec(), self.tpd.items(s2).to_vec());
        let (Some((&a, rest1)), Some((&b, rest2))) = (s1.split_first(), s2.split_first()) else {
            return Ok(false);
        };
        let pair = match (self.tpd.tree(a), self.tpd.tree(b)) {
            (T::ValDef { .. } | T::DefDef { .. }, T::ValDef { .. } | T::DefDef { .. }) => {
                self.tpd.sym(a).zip(self.tpd.sym(b))
            }
            (T::TypeDef { .. }, T::TypeDef { .. }) => return Err("a quote pattern with a type definition"),
            _ => None,
        };
        let (r1, r2) = (self.block_of(rest1, e1, s), self.block_of(rest2, e2, p));
        self.with_env(pair, |m| Ok(m.matches(a, b)? && m.matches(r1, r2)?))
    }

    /// `body` with `pair` in the environment.
    fn with_env(
        &mut self,
        pair: Option<(SymbolId, SymbolId)>,
        body: impl FnOnce(&mut Self) -> Matched,
    ) -> Matched {
        let depth = self.env.len();
        self.env.extend(pair);
        let matched = body(self);
        self.env.truncate(depth);
        matched
    }

    /// References: the same symbol through matching qualifiers, or a type tree conforming.
    fn reference(&mut self, s: TNodeId, p: TNodeId) -> Matched {
        let pt = self.tpd.tree(p).clone();
        match pt {
            T::Select { qualifier, .. } if self.same_symbol(s, p)? => match *self.tpd.tree(s) {
                T::Select { qualifier: q, .. } => self.matches(q, qualifier),
                _ if self.has_hole(qualifier) => match self.prefix_ref(s) {
                    Some(q) => self.matches(q, qualifier),
                    None => Err("a quote pattern selecting from a hole matched against an identifier"),
                },
                _ => Ok(true),
            },
            T::Ident { .. } if self.same_symbol(s, p)? => Ok(true),
            _ => Ok(self.is_type_tree(p) && self.conforms(s, p)),
        }
    }

    /// Whether `s` and `p` refer to the same symbol or to symbols the environment pairs (dotty's
    /// `symbolMatch`); refused for members of related classes of the same name.
    fn same_symbol(&mut self, s: TNodeId, p: TNodeId) -> Matched {
        let (Some(a), Some(b)) = (self.tpd.sym(s), self.tpd.sym(p)) else { return Ok(false) };
        if a == b || self.env.contains(&(a, b)) {
            return Ok(true);
        }
        let (sa, sb) = (self.u.symbol(a), self.u.symbol(b));
        let (Some(oa), Some(ob)) = (sa.owner, sb.owner) else { return Ok(false) };
        if sa.name != sb.name || !sa.kind.is_term() || !self.u.is_class(oa) || !self.u.is_class(ob) {
            return Ok(false);
        }
        let a = match *self.tpd.tree(s) {
            T::Select { qualifier, .. } => self.devirtualized(a, qualifier),
            _ => a,
        };
        Ok(a == b || self.env.contains(&(a, b)) || self.overrides(a, b))
    }

    /// The scrutinee `s` matched by a hole if it refers to no definition of the scrutinee the
    /// pattern defines too (dotty's `ClosedPatternTerm`).
    fn closed(&mut self, s: TNodeId) -> bool {
        let free = self.refers_to_env(s, &[]);
        if !free {
            self.matched.push(Hole::Closed(s));
        }
        !free
    }
}
