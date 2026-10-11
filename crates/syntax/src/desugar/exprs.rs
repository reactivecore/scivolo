//! Expressions and types the typer desugars before typing them: operators, tuples, blocks,
//! interpolated strings, symbol literals and `for`.

use super::{Desugar, Mode};
use crate::ast::constant::Constant;
use crate::ast::span::Span;
use crate::ast::{Attrs, List, NodeId, TName, Tree};

/// The largest arity of `scala.TupleN` (dotty's `MaxTupleArity`).
pub const MAX_TUPLE_ARITY: usize = 22;

impl Desugar<'_> {
    /// The desugared form of a `for`, interpolated string, symbol literal or unary operation
    /// (dotty's `desugar(tree)`), at the span of `id`.
    pub fn desugar_expr(&mut self, id: NodeId) -> NodeId {
        let t = match self.ast.tree(id).clone() {
            Tree::SymbolLit { value } => {
                let symbol = self.sym_ref("scala.Symbol", false);
                let lit = self.literal(Constant::String(value));
                self.apply(symbol, &[lit])
            }
            Tree::InterpolatedString { id: interpolator, segments } => {
                self.interpolated_string(id, interpolator, segments)
            }
            Tree::PostfixOp { od, op } => self.postfix_op(od, op),
            Tree::PrefixOp { op, od } => {
                let Tree::Ident { name } = *self.ast.tree(op) else { unreachable!("an operator") };
                let text = format!("unary_{}", self.text(name));
                let name = if self.mode == Mode::Type { self.type_name(&text) } else { self.term(&text) };
                self.select(od, name)
            }
            Tree::ForDo { enums, body } => {
                let foreach = self.term("foreach");
                let enums = self.ast.items(enums).to_vec();
                let t = self.make_for(foreach, foreach, &enums, body);
                if matches!(self.ast.tree(t), Tree::Empty) { id } else { t }
            }
            Tree::ForYield { enums, expr } => {
                let (map, flat_map) = (self.term("map"), self.term("flatMap"));
                let enums = self.ast.items(enums).to_vec();
                let t = self.make_for(map, flat_map, &enums, expr);
                if matches!(self.ast.tree(t), Tree::Empty) { id } else { t }
            }
            _ => unreachable!("a tree desugar(tree) expands"),
        };
        let span = self.ast.span(id);
        self.ast.respan(t, span)
    }

    fn interpolated_string(&mut self, id: NodeId, interpolator: TName, segments: List) -> NodeId {
        let segments = self.ast.items(segments).to_vec();
        let mut strs = Vec::new();
        let mut elems = Vec::new();
        for s in segments {
            match *self.ast.tree(s) {
                Tree::Thicket { trees } => {
                    let trees = self.ast.items(trees).to_vec();
                    strs.push(trees[0]);
                    elems.extend_from_slice(&trees[1..]);
                }
                _ => strs.push(s),
            }
        }
        let elems: Vec<NodeId> = elems
            .into_iter()
            .map(|e| match *self.ast.tree(e) {
                Tree::Block { stats, expr: None } if stats.is_empty() => self.synthetic_unit(),
                Tree::Block { stats, expr: Some(expr) } if stats.is_empty() => expr,
                _ => e,
            })
            .collect();
        let context = self.term("StringContext");
        let context = self.scala_dot(context);
        let parts = self.apply(context, &strs);
        let select = self.select(parts, interpolator);
        let span = self.ast.span(id);
        let select = self.ast.respan(select, span);
        self.apply(select, &elems)
    }

    fn postfix_op(&mut self, od: NodeId, op: NodeId) -> NodeId {
        let Tree::Ident { name } = *self.ast.tree(op) else { unreachable!("an operator") };
        let backquoted = self.ast.attrs(op).contains(Attrs::BACKQUOTED);
        if self.mode == Mode::Type && !backquoted && name.is_type && self.text(name) == "*" {
            let seq = self.sym_ref("scala.collection.immutable.Seq", true);
            let args = self.list(&[od]);
            let seq = self.ast.node(Tree::AppliedTypeTree { tpt: seq, args });
            let repeated = self.new_annot("scala.annotation.internal.Repeated");
            let init = self.term("<init>");
            let init = self.select(repeated, init);
            let annot = self.apply(init, &[]);
            self.ast.node(Tree::Annotated { arg: seq, annot })
        } else {
            let select = self.select(od, name);
            if self.text(name) == "_" {
                self.ast.add_attrs(select, Attrs::ETA_UNDERSCORE);
            }
            select
        }
    }

    /// `{ stats; () }` for a block without a result (dotty's `block`).
    pub fn block(&mut self, id: NodeId) -> NodeId {
        let Tree::Block { stats, expr: None } = *self.ast.tree(id) else { return id };
        let span = self.ast.span(id);
        let unit = self.synthetic_unit();
        let unit = self.ast.respan(unit, if stats.is_empty() { span } else { span.end_pos() });
        self.ast.derived(id, Tree::Block { stats, expr: Some(unit) })
    }

    /// `l op r` as `l.op(r)`, or `r.op(l)` for a right-associative operator (dotty's `binop`).
    pub fn binop(&mut self, left: NodeId, op: NodeId, right: NodeId) -> NodeId {
        let Tree::Ident { name } = *self.ast.tree(op) else { unreachable!("an operator") };
        let (op_span, left_span, right_span) = (self.ast.span(op), self.ast.span(left), self.ast.span(right));
        let (fun, arg, select_span) = if self.text(name).ends_with(':') {
            (right, left, Span::synthetic(op_span.start(), right_span.end()))
        } else {
            (left, right, Span::new(left_span.start(), op_span.end(), op_span.start()))
        };
        let select = self.select(fun, name);
        let select = self.ast.respan(select, select_span);
        if self.source.offset_to_line(left_span.end()) < self.source.offset_to_line(op_span.start()) {
            self.ast.add_attrs(select, Attrs::MULTI_LINE_INFIX);
        }
        let apply = match self.ast.tree(arg).clone() {
            Tree::Parens { t } => {
                let t = self.assign_to_named_arg(t);
                self.apply(select, &[t])
            }
            Tree::Tuple { trees } => {
                let args = self.ast.items(trees).to_vec();
                if args.iter().any(|&a| matches!(self.ast.tree(a), Tree::Assign { .. })) {
                    let args: Vec<NodeId> = args.iter().map(|&a| self.assign_to_named_arg(a)).collect();
                    self.apply(select, &args)
                } else {
                    let apply = self.apply(select, &[arg]);
                    self.ast.add_attrs(apply, Attrs::INFIX_TUPLE_APPLY);
                    apply
                }
            }
            _ => self.apply(select, &[arg]),
        };
        self.ast.add_attrs(apply, Attrs::WAS_TYPED_INFIX);
        apply
    }

    fn assign_to_named_arg(&mut self, arg: NodeId) -> NodeId {
        match *self.ast.tree(arg) {
            Tree::Assign { lhs, rhs } => match *self.ast.tree(lhs) {
                Tree::Ident { name } => self.ast.derived(arg, Tree::NamedArg { name, arg: rhs }),
                _ => arg,
            },
            _ => arg,
        }
    }

    /// A tuple without names as `scala.TupleN` (dotty's `tuple`); `id` itself above 22 elements.
    pub fn tuple(&mut self, id: NodeId, trees: List) -> NodeId {
        let elems = self.ast.items(trees).to_vec();
        if elems.len() > MAX_TUPLE_ARITY {
            return id;
        }
        let t = match (self.mode, elems.len()) {
            (Mode::Type, 0) => self.sym_ref("scala.Unit", true),
            (Mode::Type, n) => {
                let tpt = self.sym_ref(&format!("scala.Tuple{n}"), true);
                self.ast.node(Tree::AppliedTypeTree { tpt, args: trees })
            }
            (_, 0) => self.literal(Constant::Unit),
            (_, n) => {
                let fun = self.sym_ref(&format!("scala.Tuple{n}"), false);
                self.ast.node(Tree::Apply { fun, args: trees })
            }
        };
        let span = self.ast.span(id);
        self.ast.respan(t, span)
    }
}
