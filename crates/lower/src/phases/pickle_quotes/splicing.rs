// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Splicing.scala
//   compiler/src/dotty/tools/dotc/transform/BetaReduce.scala
//! The references of a splice's content to definitions of its quote, made parameters of the
//! content that its hole's arguments bind (dotty's `SpliceTransformer`).

use super::staged_trees::spliced;
use super::{QUOTE, SPLICE, quoted, staged};
use crate::Lower;
use crate::definitions::symbol;
use crate::phases::lazy_trees::val_def;
use crate::phases::lift_names::fresh;
use crate::phases::walk::children;
use rustc_hash::FxHashSet;
use syntax::ast::Span;
use tasty::{Flags, Kind, SymbolId, Type, TypeId};
use typer::tpd::{TNodeId, Tree};

/// The captured references of a splice's content: by first use, the original reference, which
/// is the hole's argument, and the `Expr` parameter standing for it.
#[derive(Default)]
pub struct Captures {
    pub args: Vec<TNodeId>,
    pub params: Vec<SymbolId>,
    of: Vec<SymbolId>,
}

/// The definitions of a quote and the references of one of its splices to them, with the
/// `Quotes` of the quote of level 1 being walked.
pub struct Splicer<'d> {
    pub defined: &'d FxHashSet<SymbolId>,
    pub captures: Captures,
    pub quotes: Option<TNodeId>,
}

/// Makes the references under splice content `content` in `owner` to the definitions `defined`
/// of its quote references to their `Expr` parameters: `'x` the parameter itself, `x` in a
/// nested quote its splice `${ x$1 }`, `f(a)` the splice applying `f$1` by reflection.
pub fn capture(l: &mut Lower, content: TNodeId, defined: &FxHashSet<SymbolId>, owner: SymbolId) -> Captures {
    let mut s = Splicer { defined, captures: Captures::default(), quotes: None };
    let at = l.tpd.span(content).start_pos();
    s.walk(l, (content, at), 0, owner);
    s.captures
}

impl Splicer<'_> {
    /// Transforms `t` at quote level `level` in `owner`; a tree replacing it gets the point span
    /// `at`, as dotty's copier of its parent gives it.
    pub fn walk(&mut self, l: &mut Lower, (t, at): (TNodeId, Span), level: i32, owner: SymbolId) {
        if level == 0
            && let Some(q) = quoted(l, t)
            && let Some(s) = self.captured_term(l, q.body)
        {
            let param = self.param(l, q.body, s, None);
            let reference = l.ident(param, at);
            l.copy_node(t, reference);
            return;
        }
        if let Tree::Assign { lhs, .. } = *l.tpd.tree(t)
            && self.captured_ident(l, lhs).is_some()
        {
            return;
        }
        if let Some(s) = self.captured_term(l, t) {
            let n = l.tpd.get(t);
            let (tree, span, tpe, sym) = (n.tree.clone(), n.span, n.tpe, n.sym);
            let original = l.tpd.node(tree, span, tpe, sym);
            let param = self.param(l, original, s, None);
            let tpe = expr_arg(l, param);
            let spliced = spliced(l, tpe, owner, |l, _, _| l.ident(param, Span::NONE));
            let spliced = l.positioned_from(spliced, at);
            l.copy_node(t, spliced);
            return;
        }
        if let Some(applied) = self.captured_application(l, t, level, owner) {
            let applied = l.positioned_from(applied, at);
            l.copy_node(t, applied);
            return;
        }
        let (owner, level) = match l.tpd.tree(t) {
            Tree::DefDef { .. } | Tree::ValDef { .. } => (l.tpd.sym(t).unwrap_or(owner), level),
            _ if staged(l, t, QUOTE).is_some() => (owner, level + 1),
            _ if staged(l, t, SPLICE).is_some() => (owner, level - 1),
            _ => (owner, level),
        };
        let outer = self.quotes;
        if level == 0 {
            self.quotes = quoted(l, t).map(|q| q.quotes).or(outer);
        }
        for child in children(l.tpd, t) {
            let at = child_at(l, t, child);
            self.walk(l, (child, at), level, owner);
        }
        self.quotes = outer;
    }

    /// The definition of the quote that `t` refers to if it is a reference to a value by its
    /// name.
    fn captured_term(&self, l: &mut Lower, t: TNodeId) -> Option<SymbolId> {
        let s = self.captured_ident(l, t)?;
        let tpe = value_type(l, l.tpd.tpe(t));
        let method = matches!(l.u.types().get(tpe), Type::Method { .. } | Type::Poly { .. });
        (!method).then_some(s)
    }

    /// The definition of the quote that term `t` refers to by its name.
    pub fn captured_ident(&self, l: &Lower, t: TNodeId) -> Option<SymbolId> {
        let Tree::Ident { name } = *l.tpd.tree(t) else { return None };
        l.tpd.sym(t).filter(|s| !name.is_type && self.defined.contains(s))
    }

    /// The parameter of `s`, referred to by `original`, a new one of type `Expr[tpe]` if it is the
    /// first reference, `tpe` that of `original` by default.
    pub fn param(&mut self, l: &mut Lower, original: TNodeId, s: SymbolId, tpe: Option<TypeId>) -> SymbolId {
        let c = &mut self.captures;
        if let Some(i) = c.of.iter().position(|&o| o == s) {
            return c.params[i];
        }
        let name = fresh(l, &l.u.symbol(s).name.clone());
        let owner = l.u.symbol(s).owner.expect("a local definition's owner");
        let param = l.u.define(symbol(&name, Kind::Val, owner, Flags::default()));
        let tpe = tpe.unwrap_or_else(|| value_type(l, l.tpd.tpe(original)));
        let expr = l.u.class_ref("scala.quoted.Expr");
        let info = l.u.intern_type(Type::Applied(expr, vec![tpe]));
        l.u.set_info(param, info);
        c.args.push(original);
        c.params.push(param);
        c.of.push(s);
        param
    }
}

/// The span a tree replacing `child` of `parent` gets as dotty's constructor of the parent's copy
/// gives it: the end of the siblings before, else the start of the first sibling with a span.
fn child_at(l: &Lower, parent: TNodeId, child: TNodeId) -> Span {
    l.after_siblings(parent, child).unwrap_or_else(|| {
        let siblings = children(l.tpd, parent).into_iter().filter(|&c| c != child);
        let start = siblings.map(|c| l.tpd.span(c)).filter(|s| s.exists()).map(|s| s.start()).min();
        start.map_or(l.tpd.span(parent).start_pos(), |s| Span::synthetic(s, s)).to_synthetic()
    })
}

/// `T` of the `Expr[T]` parameter `param`.
fn expr_arg(l: &mut Lower, param: SymbolId) -> TypeId {
    let info = l.u.info(param);
    l.u.applied(info).map(|(_, args)| args[0]).expect("an Expr")
}

/// `t` widened, past a by-name type.
fn value_type(l: &mut Lower, t: TypeId) -> TypeId {
    let widened = l.u.widen(t);
    match *l.u.types().get(widened) {
        Type::ByName(r) => r,
        _ => widened,
    }
}

/// `{ val p_j = contents.apply(j).asInstanceOf[Expr[T_j]]; content }` of the parameters `params`
/// of `content`, owned by `anon`, `contents` the hole's arguments (dotty's `BetaReduce` of the
/// content applied to them); the definitions at `at`, the end of the first content bound if
/// unset, as dotty shares the reference to `contents`.
pub fn bound(
    l: &mut Lower,
    (content, params): (TNodeId, &[SymbolId]),
    (contents, anon): (SymbolId, SymbolId),
    at: &mut Span,
) -> TNodeId {
    if params.is_empty() {
        return content;
    }
    let seq = l.u.class_ref("scala.collection.immutable.Seq");
    let seq = l.u.class_symbol(seq).expect("Seq");
    let apply = l.u.member(seq, "apply")[0];
    if !at.exists() {
        *at = l.tpd.span(content).end_pos().to_synthetic();
    }
    let at = *at;
    let bindings: Vec<TNodeId> = params
        .iter()
        .enumerate()
        .map(|(j, &p)| {
            l.u.set_owner(p, anon);
            let qual = l.ident(contents, Span::NONE);
            let fun = l.select(qual, apply, Span::NONE);
            let fun = l.positioned(fun);
            let index = l.int_literal(j as i32);
            let arg = l.apply(fun, &[index], tasty::ANY, Span::NONE);
            let arg = l.positioned(arg);
            let info = l.u.info(p);
            let cast = l.ensure_conforms(arg, info);
            let def = val_def(l, p, cast);
            l.with_span(def, at)
        })
        .collect();
    let tpe = l.tpd.tpe(content);
    let block = l.block(&bindings, content, Span::NONE);
    l.tpd.get_mut(block).tpe = Some(tpe);
    l.positioned(block)
}
