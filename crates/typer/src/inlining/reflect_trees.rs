// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! The trees `quotes.reflect` reads and builds.

use super::Reflect;
use crate::tpd::{MatchKind, Slot, TNodeId, Tree as T};
use syntax::ast::constant::Constant;
use syntax::ast::{Span, TName};
use tasty::tags::{INLINEPROXY, SYNTHETIC};
use tasty::{ANY, Kind, SymbolId, Type, TypeId};

impl Reflect<'_> {
    /// `Term.underlying` (dotty's `MapToUnderlying`): `t` and its subtrees without the inlined
    /// trees without bindings, empty blocks and argument names, references to local values and
    /// parameterless methods replaced by their right-hand sides, the trees changed copied.
    pub fn underlying(&mut self, t: TNodeId) -> TNodeId {
        self.mapped(t, false)
    }

    /// `Term.underlyingArgument`: `underlying` replacing only references to inline proxies and
    /// synthetic values.
    pub fn underlying_argument(&mut self, t: TNodeId) -> TNodeId {
        self.mapped(t, true)
    }

    /// `t` mapped to its underlying tree, with `proxies` for `underlyingArgument`.
    fn mapped(&mut self, t: TNodeId, proxies: bool) -> TNodeId {
        let skipped = |s: SymbolId| {
            !proxies || self.u.symbol(s).flags.has(INLINEPROXY) || self.u.symbol(s).flags.has(SYNTHETIC)
        };
        let inner = match *self.tpd.tree(t) {
            T::Inlined { bindings, expansion, .. } if bindings.is_empty() => Some(expansion),
            T::Block { stats, expr } if stats.is_empty() => Some(expr),
            T::NamedArg { arg, .. } => Some(arg),
            T::Ident { .. } => self.tpd.sym(t).filter(|&s| skipped(s)).and_then(|s| self.local_rhs(s)),
            _ => None,
        };
        if let Some(inner) = inner {
            return self.mapped(inner, proxies);
        }
        let mut tree = self.tpd.tree(t).clone();
        let mut slots = tree.slots();
        let mut changed = false;
        for slot in slots.iter_mut() {
            match slot {
                Slot::Node(c) => changed |= self.replace(c, proxies),
                Slot::Opt(Some(c)) => changed |= self.replace(c, proxies),
                Slot::List(l) => {
                    let mut items = self.tpd.items(*l).to_vec();
                    if items.iter_mut().fold(false, |any, c| self.replace(c, proxies) | any) {
                        *l = self.tpd.list(&items);
                        changed = true;
                    }
                }
                Slot::Opt(None) | Slot::Clauses(_) => {}
            }
        }
        if !changed {
            return t;
        }
        tree.set_slots(&slots);
        let node = self.tpd.get(t);
        let (span, tpe, sym) = (node.span, node.tpe, node.sym);
        self.tpd.node(tree, span, tpe, sym)
    }

    /// Replaces `c` by its underlying tree; whether that is another tree.
    fn replace(&mut self, c: &mut TNodeId, proxies: bool) -> bool {
        let new = self.mapped(*c, proxies);
        std::mem::replace(c, new) != new
    }

    /// `TypeTree.of`: a type tree of type `t`.
    pub fn type_tree(&mut self, t: TypeId) -> TNodeId {
        self.tpd.node(T::TypeTree { inferred: false }, Span::NONE, Some(t), None)
    }

    /// `Repeated(elems, elemtpt)`: a sequence literal of type `Seq[T]` (dotty's `seqLitType`).
    pub fn repeated(&mut self, elems: &[TNodeId], elemtpt: TNodeId) -> TNodeId {
        let span = self.span;
        let seq = self.u.class_ref("scala.collection.immutable.Seq");
        let tpe = self.u.intern_type(Type::Applied(seq, vec![self.tpd.tpe(elemtpt)]));
        let elems = self.tpd.list(elems);
        self.tpd.node(T::SeqLiteral { elems, elemtpt }, span, Some(tpe), None)
    }

    /// `Match(selector, cases)`, of the cases' least upper bound.
    pub fn match_(&mut self, selector: TNodeId, cases: &[TNodeId]) -> TNodeId {
        let tpe = crate::tpd::lub_of(self.tpd, self.u, cases);
        let cases = self.tpd.list(cases);
        let tree = T::Match { selector: Some(selector), cases, kind: MatchKind::Plain };
        self.tpd.node(tree, self.span, Some(tpe), None)
    }

    /// `CaseDef(pat, guard, body)`, of the body's type.
    pub fn case_def(&mut self, pat: TNodeId, guard: Option<TNodeId>, body: TNodeId) -> TNodeId {
        let tpe = self.tpd.tpe(body);
        self.tpd.node(T::CaseDef { pat, guard, body }, self.span, Some(tpe), None)
    }

    /// `Wildcard()`: the pattern `_` of type `Any`.
    pub fn wildcard(&mut self) -> TNodeId {
        let name = TName { name: self.names.intern("_"), is_type: false };
        self.tpd.node(T::Ident { name }, self.span, Some(ANY), None)
    }

    /// `Block(stats, expr)`, of the type of `expr`.
    pub fn block(&mut self, stats: &[TNodeId], expr: TNodeId) -> TNodeId {
        let (tpe, stats) = (self.tpd.tpe(expr), self.tpd.list(stats));
        self.tpd.node(T::Block { stats, expr }, self.span, Some(tpe), None)
    }

    /// The right-hand side of the definition of `s` if it is a value or parameterless method
    /// owned by a term, but for an object.
    fn local_rhs(&self, s: SymbolId) -> Option<TNodeId> {
        let sym = self.u.symbol(s);
        let local = sym.owner.is_some_and(|o| self.u.symbol(o).kind.is_term());
        if !matches!(sym.kind, Kind::Val | Kind::Def) || !local || self.u.is_module(s) {
            return None;
        }
        (0..self.tpd.len()).rev().map(TNodeId::new).find_map(|d| match *self.tpd.tree(d) {
            _ if self.tpd.sym(d) != Some(s) => None,
            T::ValDef { rhs: Some(rhs), .. } => Some(rhs),
            T::DefDef { paramss, rhs: Some(rhs), .. } if paramss.is_empty() => Some(rhs),
            _ => None,
        })
    }

    /// `Literal.constant`, if `t` is a literal.
    pub fn constant(&self, t: TNodeId) -> Option<Constant> {
        match self.tpd.tree(t) {
            T::Literal { value } => Some(value.clone()),
            _ => None,
        }
    }

    /// `Literal(constant)`.
    pub fn literal(&mut self, constant: Constant) -> Result<TNodeId, &'static str> {
        let span = self.span;
        Ok(self.build()?.literal(constant, span))
    }
}
