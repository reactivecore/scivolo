// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Spreads `xs*` among the other arguments of a repeated parameter (`experimental.multiSpreads`):
//! `<special-ops>.spread[T](xs)` in the sequence literal while typing, a `VarArgsBuilder` call
//! chain once the unit is typed (dotty's `flattenSpreads`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::defn::ARRAY;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::constant::Constant;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::{ANY, Type, TypeId};

/// The builders of `VarArgsBuilder` for elements of a primitive class.
const PRIMITIVE: [&str; 9] = ["Byte", "Short", "Char", "Int", "Long", "Float", "Double", "Boolean", "Unit"];

impl Typer<'_> {
    /// Sequence argument `a` among others passed to a repeated parameter of element type `elem`:
    /// `spread[elem](xs)`, of `pre` if overload resolution typed it.
    pub fn spread_arg(&mut self, a: NodeId, pre: Option<TNodeId>, elem: TypeId, ctx: Ctx) -> TNodeId {
        let typed = self.pretyped_sequence_arg(a, pre, elem, ctx);
        let T::Typed { expr, .. } = *self.tpd.tree(typed) else { return typed };
        let spread = crate::special::spread_method(self.u);
        let span = self.tpd.span(expr);
        let at = span.start_pos();
        let fun_tpe = self.u.intern_type(Type::Sym(None, spread));
        let fun = self.build().ident(spread, fun_tpe, at);
        let targ = self.type_tree(elem, at, true);
        let binder = self.u.fresh_binder();
        let params = [(self.u.intern_name("x$0"), ANY)];
        let info = self.u.intern_method(binder, &params, elem, false, false);
        let fun = self.build().type_apply(fun, &[targ], info, at);
        self.build().apply(fun, &[expr], elem, span.to_synthetic())
    }

    /// The spread `spread(xs)` makes of `xs`, if `t` is one.
    fn spread_of(&self, t: TNodeId) -> Option<TNodeId> {
        let T::Apply { fun, args } = *self.tpd.tree(t) else { return None };
        let s = self.tpd.sym(self.tpd.meth_part(fun))?;
        let ops = self.u.symbol(s).owner.is_some_and(|o| self.u.symbol(o).name == "<special-ops>");
        let &[xs] = self.tpd.items(args) else { return None };
        (ops && self.u.symbol(s).name == "spread").then_some(xs)
    }

    /// Records sequence literal `seq`, typed in `ctx`, if it has spreads; its element type tree
    /// is at the first one's start, as the one tree dotty shares with them.
    pub fn note_spreads(&mut self, seq: TNodeId, ctx: Ctx) {
        let T::SeqLiteral { elems, elemtpt } = *self.tpd.tree(seq) else { return };
        let spreads: Vec<TNodeId> =
            self.tpd.items(elems).iter().copied().filter(|&e| self.spread_of(e).is_some()).collect();
        let Some(&first) = spreads.first() else { return };
        let at = self.tpd.span(first).start_pos();
        self.tpd.get_mut(elemtpt).span = at;
        for &s in &spreads[1..] {
            let T::Apply { fun, .. } = *self.tpd.tree(s) else { continue };
            let end = self.tpd.span(s).end();
            self.tpd.get_mut(s).span = Span::synthetic(at.start(), end);
            let mut parts = vec![fun];
            while let Some(p) = parts.pop() {
                self.tpd.get_mut(p).span = at;
                parts.extend(self.tpd.children(p));
            }
        }
        self.spread_ctxs.insert(seq, ctx);
    }

    /// Replaces each sequence literal with spreads under `root` by its `VarArgsBuilder` chain.
    pub fn flatten_spreads(&mut self, root: TNodeId) {
        let mut found = Vec::new();
        let ctxs = &self.spread_ctxs;
        self.tpd.fold((), root, &mut |(), t| {
            if let Some(&ctx) = ctxs.get(&t) {
                found.push((t, ctx));
            }
        });
        for (seq, ctx) in found {
            match self.spreads_flattened(seq, ctx) {
                Ok(chain) => {
                    let node = self.tpd.get(chain);
                    let (tree, tpe) = (node.tree.clone(), node.tpe);
                    let node = self.tpd.get_mut(seq);
                    (node.tree, node.tpe) = (tree, tpe);
                }
                Err(what) => self.unsupported_at(self.tpd.span(seq), what),
            }
        }
    }

    /// `VarArgsBuilder.ofT(n + xs.length + ...).add(x).addSeq(xs)...result()` for sequence
    /// literal `seq` with spreads, typed in `ctx`.
    fn spreads_flattened(&mut self, seq: TNodeId, ctx: Ctx) -> Result<TNodeId, &'static str> {
        let T::SeqLiteral { elems, elemtpt } = *self.tpd.tree(seq) else { return Err("spread") };
        let elems = self.tpd.items(elems).to_vec();
        let span = self.tpd.span(seq).to_synthetic();
        let elem = self.tpd.tpe(elemtpt);
        let mut items: Vec<(TNodeId, bool)> =
            elems.iter().map(|&e| self.spread_of(e).map_or((e, false), |xs| (xs, true))).collect();
        let mut defs = Vec::new();
        if items.iter().any(|&(xs, spread)| spread && !self.is_idempotent_expr(xs)) {
            for (e, _) in &mut items {
                if !self.is_idempotent_expr(*e) {
                    *e = self.lift(&mut defs, *e, "ev", false, ctx);
                }
            }
        }
        let spreads: Vec<TNodeId> = items.iter().filter(|&&(_, spread)| spread).map(|&(xs, _)| xs).collect();
        let singles = (elems.len() - spreads.len()) as i32;
        let mut total = self.ast.node(Tree::Literal { value: Constant::Int(singles) });
        self.ast.with_span(total, span);
        for xs in spreads {
            let xs = self.spliced(xs);
            let len = self.spread_select(xs, "length", span);
            let plus = self.spread_select(total, "+", span);
            total = self.spread_applied(plus, &[len], span);
        }
        let class = self.u.class_symbol(elem);
        let primitive = class.map(|c| self.u.full_name(c)).and_then(|n| {
            PRIMITIVE.iter().find(|&&p| n.strip_prefix("scala.") == Some(p)).map(|p| format!("of{p}"))
        });
        let object = self.u.defn().object;
        let reference = class.is_some_and(|c| self.u.derives_from(c, object));
        let module = TName { name: self.name("scala.runtime.VarArgsBuilder"), is_type: false };
        let module = self.ast.node(Tree::TypedSplice { sym: module });
        self.ast.with_span(module, span);
        let builder = match primitive {
            Some(name) => self.spread_select(module, &name, span),
            None => {
                let fun = self.spread_select(module, if reference { "ofRef" } else { "generic" }, span);
                self.spread_type_applied(fun, elem, span)
            }
        };
        let mut chain = self.spread_applied(builder, &[total], span);
        for (e, spread) in items {
            let (method, arg) = match spread {
                true => self.spread_added(e, elem),
                false => ("add", self.spliced(e)),
            };
            let add = self.spread_select(chain, method, span);
            chain = self.spread_applied(add, &[arg], span);
        }
        let result = self.spread_select(chain, "result", span);
        let result = self.spread_applied(result, &[], span);
        let unsupported = self.unsupported.take();
        let mut typed = self.typed(result, Pt::Any, ctx);
        let refused = std::mem::replace(&mut self.unsupported, unsupported);
        if !defs.is_empty() {
            let stats = self.tpd.list(&defs);
            typed = self.build().block(stats, typed, span);
        }
        let zonked = self.zonk_trees();
        match refused.is_some() || self.diags.has_errors() || zonked.is_err() {
            true => Err("spread the typer cannot flatten"),
            false => Ok(typed),
        }
    }

    /// The `VarArgsBuilder` method that adds the elements of spread `xs` of elements `elem`,
    /// and `xs` cast to `Array[elem]` if it is an array of another type.
    fn spread_added(&mut self, xs: TNodeId, elem: TypeId) -> (&'static str, NodeId) {
        let tpe = self.u.widen(self.tpd.tpe(xs));
        let seq = self.resolve_full_name("scala.collection.Seq", true);
        let span = self.tpd.span(xs);
        let xs_node = self.spliced(xs);
        if self.u.class_symbol(tpe).zip(seq).is_some_and(|(c, s)| self.u.derives_from(c, s)) {
            return ("addSeq", xs_node);
        }
        let array = self.u.class_ref(ARRAY);
        let array = self.u.intern_applied(array, &[elem]);
        if self.conforms(tpe, array) {
            return ("addArray", xs_node);
        }
        let fun = self.spread_select(xs_node, "asInstanceOf", span.to_synthetic());
        ("addArray", self.spread_type_applied(fun, array, span.to_synthetic()))
    }

    fn spread_type_applied(&mut self, fun: NodeId, targ: TypeId, span: Span) -> NodeId {
        let targ = self.type_tree(targ, span.end_pos(), true);
        let targs = [self.spliced(targ)];
        let args = self.ast.list(&targs);
        let applied = self.ast.node(Tree::TypeApply { fun, args });
        self.ast.with_span(applied, span);
        applied
    }

    fn spread_select(&mut self, qualifier: NodeId, name: &str, span: Span) -> NodeId {
        let name = TName { name: self.name(name), is_type: false };
        let select = self.ast.node(Tree::Select { qualifier, name });
        self.ast.with_span(select, span);
        select
    }

    fn spread_applied(&mut self, fun: NodeId, args: &[NodeId], span: Span) -> NodeId {
        let args = self.ast.list(args);
        let apply = self.ast.node(Tree::Apply { fun, args });
        self.ast.with_span(apply, span);
        apply
    }
}
