// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//! Function values of several parameters where a function or a single abstract method of one
//! tuple is expected, as `x$1 => { val a = x$1._1; ...; body }`, `x$1.apply(0)` for a generic
//! tuple (dotty's `ptIsCorrectProduct`, `makeTupledFunction`).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use syntax::ast::constant::Constant;
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{List, NodeId, Span, TName, Tree};
use tasty::Type;

impl Typer<'_> {
    /// Function value `id` of parameters `args` and `body` untupled against `pt`, if `pt` is a
    /// function of one tuple of as many elements as there are parameters, each conforming to the
    /// parameter's declared type.
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
        let formals = match self.function_parts(p, 1) {
            Some((formals, _)) => formals,
            None => self.sam_parts(p, 1, true).ok()??.params,
        };
        let formal = self.zonk(formals[0]);
        if matches!(self.u.types().get(formal), Type::Bounds(..)) {
            return Some(self.unsupported(id, "untupled parameters of a wildcard formal"));
        }
        let tuple = self.u.dealias(formal);
        let Type::Applied(tycon, args) = self.u.types().get(tuple).clone() else { return None };
        let class = self.u.class_symbol(tycon)?;
        let class = self.u.full_name(class);
        let (elems, generic) = match class.as_str() {
            "scala.*:" => (self.tuple_elements_within(formal, usize::MAX)?, true),
            _ if class == format!("scala.Tuple{}", args.len()) => (args, false),
            _ => return None,
        };
        if elems.len() != params.len() {
            return None;
        }
        if formal != tuple {
            return Some(self.unsupported(id, "untupled parameters of a tuple alias"));
        }
        if !self.is_fully_defined(formal) {
            return Some(self.unsupported(id, "untupled parameters with type variables"));
        }
        let at = |s: Span| Span::synthetic(s.start(), s.start());
        for (&q, &elem) in params.iter().zip(&elems) {
            let Tree::ValDef { tpt, .. } = *self.ast.tree(q) else { return None };
            if self.is_inferred_tpt(tpt) {
                continue;
            }
            let declared = self.typed_type(tpt, ctx);
            let declared = self.tpd.tpe(declared);
            if !self.u.is_subtype(elem, declared) {
                return None;
            }
        }
        let x = TName { name: self.name("x$1"), is_type: false };
        let tpt = self.tuple_of_declared(&params);
        let mods = self.ast.mods(Modifiers::with_flags(f::SYNTHETIC_TERM_PARAM));
        let param = self.ast.node(Tree::ValDef { name: x, tpt, rhs: None, mods });
        let mut vdefs = Vec::new();
        for (i, &q) in params.iter().enumerate() {
            let Tree::ValDef { name, tpt: declared, .. } = *self.ast.tree(q) else { return None };
            let text = self.text(name.name);
            let used = || {
                self.ast
                    .exists_subtree(body, &|a, t| matches!(*a.tree(t), Tree::Ident { name: n } if n == name))
            };
            if text == "_" || (text.starts_with("_$") && !used()) {
                continue;
            }
            let q_span = self.ast.span(q);
            let (tpt, rhs_at) = if self.is_inferred_tpt(declared) {
                let tpt_span = self.ast.span(declared);
                let rhs_at = if tpt_span == Span::NONE { at(q_span) } else { at(tpt_span) };
                let tpt = self.ast.node(Tree::TypeTree);
                self.ast.with_span(tpt, rhs_at);
                (tpt, rhs_at)
            } else {
                (declared, Span::synthetic(q_span.end(), q_span.end()))
            };
            let ident = self.ast.node(Tree::Ident { name: x });
            self.ast.with_span(ident, rhs_at);
            let selector = if generic { "apply".to_owned() } else { format!("_{}", i + 1) };
            let selector = TName { name: self.name(&selector), is_type: false };
            let mut select = self.ast.node(Tree::Select { qualifier: ident, name: selector });
            self.ast.with_span(select, rhs_at);
            if generic {
                let index = self.ast.node(Tree::Literal { value: Constant::Int(i as i32) });
                self.ast.with_span(index, rhs_at);
                let args = self.ast.list(&[index]);
                select = self.ast.node(Tree::Apply { fun: select, args });
                self.ast.with_span(select, rhs_at);
            }
            let mods = self.ast.mods(Modifiers::default());
            let vdef = self.ast.node(Tree::ValDef { name, tpt, rhs: Some(select), mods });
            self.ast.with_span(vdef, q_span);
            vdefs.push(vdef);
        }
        let first = vdefs.first().copied().unwrap_or(body);
        let span = Span::synthetic(self.ast.span(first).start(), self.ast.span(id).end());
        match *self.ast.tree(tpt) {
            Tree::TypeTree => self.ast.with_span(param, at(span)),
            _ => self.ast.with_span(param, self.ast.span(tpt)),
        }
        let stats = self.ast.list(&vdefs);
        let block = self.ast.node(Tree::Block { stats, expr: Some(body) });
        self.ast.with_span(block, span);
        let args = self.ast.list(&[param]);
        let function = self.ast.node(Tree::Function { args, body: block, mods: None, wildcard: false });
        self.ast.with_span(function, span);
        Some(self.typed_function(function, args, block, pt, ctx))
    }

    /// The type `(T1, ..., Tn)` of the declared types of `params`, an inferred type tree unless
    /// each has one.
    fn tuple_of_declared(&mut self, params: &[NodeId]) -> NodeId {
        let mut tpts = Vec::new();
        for &q in params {
            match *self.ast.tree(q) {
                Tree::ValDef { tpt, .. } if !self.is_inferred_tpt(tpt) => tpts.push(tpt),
                _ => return self.ast.node(Tree::TypeTree),
            }
        }
        if tpts.len() > super::small_tuples::MAX_TUPLE_ARITY {
            return self.ast.node(Tree::TypeTree);
        }
        let (first, last) = (self.ast.span(tpts[0]), self.ast.span(tpts[tpts.len() - 1]));
        let tycon = TName { name: self.name(&format!("scala.Tuple{}", tpts.len())), is_type: true };
        let tycon = self.ast.node(Tree::TypedSplice { sym: tycon });
        self.ast.with_span(tycon, Span::synthetic(first.start(), first.start()));
        let args = self.ast.list(&tpts);
        let tuple = self.ast.node(Tree::AppliedTypeTree { tpt: tycon, args });
        self.ast.with_span(tuple, Span::synthetic(first.start(), last.end()));
        tuple
    }
}
