// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! The trees `quotes.reflect` reads and builds.

use super::Reflect;
use crate::tpd::{Slot, TNodeId, Tree as T};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Reflect<'_> {
    /// `Term.underlying` (dotty's `underlyingArgument` at the top of `t`): `t` without the
    /// inlined trees without bindings, empty blocks and argument names around it, a reference to
    /// a local value replaced by the value's right-hand side.
    pub fn underlying(&self, t: TNodeId) -> TNodeId {
        match *self.tpd.tree(t) {
            T::Inlined { bindings, expansion, .. } if bindings.is_empty() => self.underlying(expansion),
            T::Block { stats, expr } if stats.is_empty() => self.underlying(expr),
            T::NamedArg { arg, .. } => self.underlying(arg),
            T::Ident { .. } => match self.tpd.sym(t).and_then(|s| self.local_rhs(s)) {
                Some(rhs) => self.underlying(rhs),
                None => t,
            },
            _ => t,
        }
    }

    /// `Term.underlyingArgument` (dotty's `MapToUnderlying`): `underlying` applied to `t` and to
    /// every subtree, the trees changed copied.
    pub fn underlying_argument(&mut self, t: TNodeId) -> TNodeId {
        let t = self.underlying(t);
        let mut tree = self.tpd.tree(t).clone();
        let mut slots = tree.slots();
        let mut changed = false;
        for slot in slots.iter_mut() {
            match slot {
                Slot::Node(c) => changed |= self.replace(c),
                Slot::Opt(Some(c)) => changed |= self.replace(c),
                Slot::List(l) => {
                    let mut items = self.tpd.items(*l).to_vec();
                    if items.iter_mut().fold(false, |any, c| self.replace(c) | any) {
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

    /// Replaces `c` by its underlying argument; whether that is another tree.
    fn replace(&mut self, c: &mut TNodeId) -> bool {
        let new = self.underlying_argument(*c);
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

    /// The right-hand side of the definition of `s` if it is a value owned by a term.
    fn local_rhs(&self, s: SymbolId) -> Option<TNodeId> {
        let sym = self.u.symbol(s);
        let local = sym.owner.is_some_and(|o| self.u.symbol(o).kind.is_term());
        if sym.kind != Kind::Val || !local {
            return None;
        }
        (0..self.tpd.len()).rev().map(TNodeId::new).find_map(|d| match *self.tpd.tree(d) {
            T::ValDef { rhs: Some(rhs), .. } if self.tpd.sym(d) == Some(s) => Some(rhs),
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
