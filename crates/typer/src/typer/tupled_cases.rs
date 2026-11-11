// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Pattern-matching lambdas of a tuple pattern as functions of several parameters (dotty's
//! `typedFunctionValue`, `patternsToParams`).

use super::{Pt, Typer};
use names::name_kinds;
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::tree::MatchKind;
use syntax::ast::{Attrs, List, NodeId, Span, TName, Tree};

impl Typer<'_> {
    /// `x$1 => x$1 match { case (a1, …, aN) => e }` of a synthetic parameter `x$1` as
    /// `(a1, …, aN) => e` where `pt` is a function or single abstract method type of N > 1
    /// parameters and each `ai` a name, possibly typed, else as a case lambda of N parameters;
    /// its parameters and body.
    pub fn untupled_case(&mut self, params: &[NodeId], body: NodeId, pt: Pt) -> Option<(List, NodeId)> {
        let (&[param], Pt::Type(p)) = (params, pt) else { return None };
        let Tree::ValDef { name, mods, .. } = *self.ast.tree(param) else { return None };
        if !self.ast.modifiers(mods).flags.is(f::SYNTHETIC) {
            return None;
        }
        let Tree::Match { selector: Some(mut sel), cases, .. } = *self.ast.tree(body) else { return None };
        while let Tree::Annotated { arg, .. } = *self.ast.tree(sel) {
            sel = arg;
        }
        let (&Tree::Ident { name: n }, &[case]) = (self.ast.tree(sel), self.ast.items(cases)) else {
            return None;
        };
        let Tree::CaseDef { pat, guard: None, body: rhs } = *self.ast.tree(case) else { return None };
        let Tree::Apply { fun, args } = *self.ast.tree(pat) else { return None };
        let elems = self.ast.items(args).to_vec();
        let tuple = matches!(*self.ast.tree(fun), Tree::TypedSplice { sym }
            if !sym.is_type && self.text(sym.name) == format!("scala.Tuple{}", elems.len()));
        if !tuple || n != name || elems.len() < 2 {
            return None;
        }
        let z = self.zonk(p);
        let p = self.function_type_in(z).unwrap_or(z);
        if self.function_parts(p, elems.len()).is_none()
            && !matches!(self.sam_parts(p, elems.len(), true), Ok(Some(_)))
        {
            return None;
        }
        let params: Option<Vec<NodeId>> =
            elems.iter().map(|&e| self.pattern_param(e, None, self.ast.span(e))).collect();
        let Some(params) = params else {
            return Some(self.case_lambda_of((param, sel), cases, elems.len()));
        };
        Some((self.ast.list(&params), rhs))
    }

    /// `(x$1, …, x$N) => (x$1, …, x$N) match { cases }` at the spans of the parameter and the
    /// selector `sel` of the one-parameter lambda (dotty's `makeCaseLambda`).
    fn case_lambda_of(&mut self, (param, sel): (NodeId, NodeId), cases: List, n: usize) -> (List, NodeId) {
        let Tree::ValDef { tpt, mods, .. } = *self.ast.tree(param) else { unreachable!("a parameter") };
        let (param_span, sel_span) = (self.ast.span(param), self.ast.span(sel));
        let mut params = Vec::new();
        let mut idents = Vec::new();
        for i in 1..=n {
            let name = TName { name: self.name(&name_kinds::synthetic_param(i)), is_type: false };
            let tpt = match i {
                1 => tpt,
                _ => self.ast.node(Tree::TypeTree),
            };
            let p = self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods });
            self.ast.get_mut(p).span = param_span;
            params.push(p);
            let ident = self.ast.node(Tree::Ident { name });
            self.ast.get_mut(ident).span = sel_span;
            idents.push(ident);
        }
        let sym = TName { name: self.name(&format!("scala.Tuple{n}")), is_type: false };
        let fun = self.ast.node(Tree::TypedSplice { sym });
        let args = self.ast.list(&idents);
        let selector = self.ast.node(Tree::Apply { fun, args });
        self.ast.get_mut(fun).span = sel_span;
        self.ast.get_mut(selector).span = sel_span;
        let check = match self.ast.attrs(sel).contains(Attrs::CHECK_GEN_FROM) {
            true => Attrs::CHECK_GEN_FROM,
            false => Attrs::CHECK_PAT_DEF,
        };
        self.ast.add_attrs(selector, check);
        let body = self.ast.node(Tree::Match { selector: Some(selector), cases, kind: MatchKind::Plain });
        (self.ast.list(&params), body)
    }

    /// Pattern `elem`, a name possibly typed or annotated, as a parameter of type tree `tpt` at
    /// `span`.
    fn pattern_param(&mut self, elem: NodeId, tpt: Option<NodeId>, span: Span) -> Option<NodeId> {
        match *self.ast.tree(elem) {
            Tree::Annotated { arg, .. } => self.pattern_param(arg, tpt, span),
            Tree::Typed { expr, tpt } => self.pattern_param(expr, Some(tpt), span),
            Tree::Ident { name } if !name.is_type => {
                let tpt = tpt.unwrap_or_else(|| self.ast.node(Tree::TypeTree));
                let mods = self.ast.mods(Modifiers::with_flags(f::TERM_PARAM));
                let param = self.ast.node(Tree::ValDef { name, tpt, rhs: None, mods });
                self.ast.get_mut(param).span = span;
                Some(param)
            }
            _ => None,
        }
    }
}
