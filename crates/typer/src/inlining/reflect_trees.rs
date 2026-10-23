// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! The trees `quotes.reflect` reads and builds.

use super::Reflect;
use crate::tpd::{Slot, TNodeId, Tree as T};
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use tasty::tags::{GIVEN, IMPLICIT, INLINEPROXY, LAZY, MUTABLE, SYNTHETIC};
use tasty::{Flags, Kind, Symbol, SymbolId, Type, TypeId};

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

    /// The definition of a synthetic value `name` of `owner` with right-hand side `rhs` and the
    /// TASTy modifier bits `flags` (dotty's `SyntheticValDef`), and a reference to it.
    pub fn let_(
        &mut self,
        owner: SymbolId,
        name: &str,
        rhs: TNodeId,
        flags: u64,
    ) -> Result<(TNodeId, TNodeId), &'static str> {
        let allowed = [LAZY, MUTABLE, SYNTHETIC, IMPLICIT, GIVEN].iter().fold(0, |b, &t| b | 1u64 << t);
        if flags & !allowed != 0 {
            return Err("ValDef.let with flags other than Lazy, Mutable, Synthetic, Implicit and Given");
        }
        let span = self.span;
        let info = self.tpd.get(rhs).tpe.ok_or("ValDef.let of a term without a type")?;
        let info = self.u.widen(info);
        let s = self.u.define(Symbol {
            name: name.to_owned(),
            kind: Kind::Val,
            owner: None,
            flags: Flags(flags).with(SYNTHETIC),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        self.u.set_owner(s, owner);
        self.u.set_info(s, info);
        let tpt = self.type_tree(info);
        let mut b = self.build()?;
        let vdef = b.val_def(s, tpt, Some(rhs), span);
        Ok((vdef, b.static_ident(s, span)))
    }

    /// `Block(stats, expr)`, of the type of `expr` widened if it refers to a definition of `stats`;
    /// refused if the widened type still does.
    pub fn block(&mut self, stats: &[TNodeId], expr: TNodeId) -> Result<TNodeId, &'static str> {
        let span = self.span;
        let defined: Vec<SymbolId> = stats.iter().filter_map(|&s| self.tpd.sym(s)).collect();
        let local = |u: &mut tasty::Universe, t: TypeId| {
            let mut found = false;
            u.map_type(t, &mut |u, part| {
                found |= matches!(*u.types().get(part), Type::Sym(_, s) if defined.contains(&s));
                None
            });
            found
        };
        let mut tpe = self.tpd.tpe(expr);
        if local(self.u, tpe) {
            tpe = self.u.widen(tpe);
            if local(self.u, tpe) {
                return Err("Block.apply of an expression whose type refers to the block's definitions");
            }
        }
        let stats = self.tpd.list(stats);
        Ok(self.tpd.node(T::Block { stats, expr }, span, Some(tpe), None))
    }

    /// `Literal(constant)`.
    pub fn literal(&mut self, constant: Constant) -> Result<TNodeId, &'static str> {
        let span = self.span;
        Ok(self.build()?.literal(constant, span))
    }
}
