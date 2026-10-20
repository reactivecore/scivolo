// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//! Function values of several parameters where a function of one tuple is expected, as
//! `x$1 => { val a = x$1._1; ...; body }` (dotty's `makeTupledFunction`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{List, NodeId, Span, TName, Tree};
use tasty::Type;

impl Typer<'_> {
    /// Function value `id` of parameters `args` and `body` untupled against `pt`, if `pt` is a
    /// function of one tuple of as many elements as there are parameters.
    pub fn untupled_function(
        &mut self,
        id: NodeId,
        args: List,
        body: NodeId,
        pt: Pt,
        ctx: Ctx,
    ) -> Option<TNodeId> {
        let params = self.ast.items(args).to_vec();
        let Pt::Type(p) = pt else { return None };
        if params.len() < 2 {
            return None;
        }
        let (formals, _) = self.function_parts(p, 1)?;
        let formal = self.zonk(formals[0]);
        if matches!(self.u.types().get(formal), Type::Bounds(..)) {
            return Some(self.unsupported(id, "untupled parameters of a wildcard formal"));
        }
        let tuple = self.u.dealias(formal);
        let Type::Applied(tycon, elems) = self.u.types().get(tuple).clone() else { return None };
        let class = self.u.class_symbol(tycon)?;
        if self.u.full_name(class) != format!("scala.Tuple{}", params.len()) || elems.len() != params.len() {
            return None;
        }
        if formal != tuple {
            return Some(self.unsupported(id, "untupled parameters of a tuple alias"));
        }
        let at = |s: Span| Span::synthetic(s.start(), s.start());
        let declared = params.iter().any(|&q| match *self.ast.tree(q) {
            Tree::ValDef { tpt, .. } => !self.is_inferred_tpt(tpt),
            _ => true,
        });
        if declared || !self.is_fully_defined(formal) {
            return Some(self.unsupported(id, "untupled parameters with declared types or type variables"));
        }
        let x = TName { name: self.name("x$1"), is_type: false };
        let tpt = self.ast.node(Tree::TypeTree);
        let mods = self.ast.mods(Modifiers::with_flags(f::SYNTHETIC_TERM_PARAM));
        let param = self.ast.node(Tree::ValDef { name: x, tpt, rhs: None, mods });
        let mut vdefs = Vec::new();
        for (i, &q) in params.iter().enumerate() {
            let Tree::ValDef { name, .. } = *self.ast.tree(q) else { return None };
            let text = self.text(name.name);
            let used = || {
                self.ast
                    .exists_subtree(body, &|a, t| matches!(*a.tree(t), Tree::Ident { name: n } if n == name))
            };
            if text == "_" || (text.starts_with("_$") && !used()) {
                continue;
            }
            let q_span = self.ast.span(q);
            let tpt = self.ast.node(Tree::TypeTree);
            self.ast.with_span(tpt, at(q_span));
            let ident = self.ast.node(Tree::Ident { name: x });
            self.ast.with_span(ident, at(q_span));
            let selector = TName { name: self.name(&format!("_{}", i + 1)), is_type: false };
            let select = self.ast.node(Tree::Select { qualifier: ident, name: selector });
            self.ast.with_span(select, at(q_span));
            let mods = self.ast.mods(Modifiers::default());
            let vdef = self.ast.node(Tree::ValDef { name, tpt, rhs: Some(select), mods });
            self.ast.with_span(vdef, q_span);
            vdefs.push(vdef);
        }
        let first = vdefs.first().copied().unwrap_or(body);
        let span = Span::synthetic(self.ast.span(first).start(), self.ast.span(id).end());
        self.ast.with_span(param, at(span));
        let stats = self.ast.list(&vdefs);
        let block = self.ast.node(Tree::Block { stats, expr: Some(body) });
        self.ast.with_span(block, span);
        let args = self.ast.list(&[param]);
        let function = self.ast.node(Tree::Function { args, body: block, mods: None, wildcard: false });
        self.ast.with_span(function, span);
        Some(self.typed_function(function, args, block, pt, ctx))
    }
}
