//! dotty's `TailRecElimination`: each call of a method to itself in tail position becomes the
//! assignment of its arguments to variables for the receiver and the parameters, and a jump.

use super::lazy_trees::{assign, local, return_, val_def};
use super::tailrec::fresh;
use super::walk::children;
use crate::Lower;
use syntax::ast::Span;
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// The rewriting of the tail calls of `method` of `class`, whose parameters are `params`.
pub struct TailCalls {
    pub method: SymbolId,
    pub class: SymbolId,
    pub params: Vec<SymbolId>,
    /// The label a tail call jumps to, once one is rewritten.
    pub label: Option<SymbolId>,
    /// The variable for the receiver, if a tail call changes it.
    pub this_var: Option<SymbolId>,
    /// The parameters a tail call changes with their variables, the last one made first.
    pub vars: Vec<(SymbolId, SymbolId)>,
    /// The labeled blocks in tail position around the tree walked.
    tail_labels: Vec<SymbolId>,
}

impl TailCalls {
    pub fn new(method: SymbolId, class: SymbolId, params: Vec<SymbolId>) -> Self {
        TailCalls {
            method,
            class,
            params,
            label: None,
            this_var: None,
            vars: Vec::new(),
            tail_labels: Vec::new(),
        }
    }

    /// Rewrites the tail calls under `t`, in tail position if `tail`.
    pub fn walk(&mut self, l: &mut Lower, t: TNodeId, tail: bool) {
        match *l.tpd.tree(t) {
            Tree::Apply { fun, args } if is_boolean_op(l, fun) => {
                self.walk(l, fun, false);
                for a in l.tpd.items(args).to_vec() {
                    self.walk(l, a, tail);
                }
            }
            Tree::Apply { .. } => self.apply(l, t, tail),
            Tree::Select { qualifier, .. } => self.walk(l, qualifier, false),
            Tree::Block { stats, expr } => {
                for s in l.tpd.items(stats).to_vec() {
                    self.walk(l, s, false);
                }
                self.walk(l, expr, tail);
            }
            Tree::If { cond, thenp, elsep, .. } => {
                self.walk(l, cond, false);
                self.walk(l, thenp, tail);
                self.walk(l, elsep, tail);
            }
            Tree::CaseDef { body, .. } => self.walk(l, body, tail),
            Tree::Match { selector, cases, .. } => {
                self.walk(l, selector, false);
                for c in l.tpd.items(cases).to_vec() {
                    self.walk(l, c, tail);
                }
            }
            Tree::Try { expr, cases, finalizer } => {
                self.walk(l, expr, false);
                for c in l.tpd.items(cases).to_vec() {
                    self.walk(l, c, tail && finalizer.is_none());
                }
                if let Some(f) = finalizer {
                    self.walk(l, f, false);
                }
            }
            Tree::WhileDo { cond, body } => {
                self.walk(l, cond, false);
                self.walk(l, body, false);
            }
            Tree::ValDef { rhs: Some(rhs), .. } => self.walk(l, rhs, false),
            Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. } => {}
            Tree::Labeled { expr } => {
                let label = l.tpd.sym(t).expect("a label");
                if tail {
                    self.tail_labels.push(label);
                }
                self.walk(l, expr, tail);
                if tail {
                    self.tail_labels.pop();
                }
            }
            Tree::Return { expr, from } => {
                let from = l.tpd.sym(from);
                let tail = from.is_some_and(|f| f == self.method || self.tail_labels.contains(&f));
                self.walk(l, expr, tail);
            }
            _ => {
                for c in children(l.tpd, t) {
                    self.walk(l, c, tail);
                }
            }
        }
    }

    /// Rewrites call `t` if it calls the method in tail position (dotty's `rewriteApply`).
    fn apply(&mut self, l: &mut Lower, t: TNodeId, tail: bool) {
        let (mut fun, mut argss) = (t, Vec::new());
        while let Tree::Apply { fun: f, args } = *l.tpd.tree(fun) {
            argss.push(l.tpd.items(args).to_vec());
            fun = f;
        }
        while let Tree::TypeApply { fun: f, .. } = *l.tpd.tree(fun) {
            fun = f;
        }
        let args: Vec<TNodeId> = argss.into_iter().rev().flatten().collect();
        for &a in &args {
            self.walk(l, a, false);
        }
        let prefix = match *l.tpd.tree(fun) {
            Tree::Select { qualifier, .. } => Some(qualifier),
            _ => None,
        };
        let recursive = l.tpd.sym(fun) == Some(self.method)
            && (prefix.is_some() || matches!(l.tpd.tree(fun), Tree::Ident { .. }));
        if !recursive || !tail {
            self.walk(l, fun, false);
            return;
        }
        let mut pairs = Vec::new();
        for (&param, &arg) in self.params.clone().iter().zip(&args) {
            if !(matches!(l.tpd.tree(arg), Tree::Ident { .. }) && l.tpd.sym(arg) == Some(param)) {
                pairs.push((self.param_var(l, param), arg));
            }
        }
        if let Some(q) = prefix.filter(|&q| !self.is_this(l, q)) {
            self.walk(l, q, false);
            pairs.insert(0, (self.this_var(l), q));
        }
        let span = l.tpd.span(t);
        let stats = match &pairs[..] {
            [(var, rhs)] => vec![assign(l, *var, *rhs)],
            _ => {
                let (mut defs, mut assigns) = (Vec::new(), Vec::new());
                for (var, rhs) in pairs {
                    let name = fresh(l, &format!("{}$tmp", l.u.symbol(var).name));
                    let info = l.u.info(var);
                    let temp = local(l, self.method, &name, info, false);
                    defs.push(val_def(l, temp, rhs));
                    let read = l.ident(temp, Span::NONE);
                    let a = assign(l, var, read);
                    assigns.push(l.with_span(a, span));
                }
                defs.into_iter().chain(assigns).collect()
            }
        };
        let label = self.label(l);
        let unit = l.unit_literal(Span::NONE);
        let unit = l.with_span(unit, span);
        let jump = return_(l, label, unit);
        let info = l.u.info(self.method);
        let result = l.u.result_type(info);
        let tpt = l.type_tree(result, Span::NONE);
        let typed = l.tpd.node(Tree::Typed { expr: jump, tpt }, Span::NONE, Some(result), None);
        let mut new = l.positioned(typed);
        if !stats.is_empty() {
            let block = l.block(&stats, new, Span::NONE);
            new = l.positioned(block);
        }
        l.copy_node(t, new);
    }

    /// Whether receiver `q` is `this` of the method's class; a reference to an object is not, as
    /// it is not a pure path (dotty's `isPurePath` of a module).
    fn is_this(&self, l: &Lower, q: TNodeId) -> bool {
        matches!(l.tpd.tree(q), Tree::This { .. }) && l.tpd.sym(q) == Some(self.class)
    }
}

/// Whether `fun` is `Boolean`'s `||` or `&&`.
fn is_boolean_op(l: &Lower, fun: TNodeId) -> bool {
    l.tpd.sym(fun).is_some_and(|m| {
        let s = l.u.symbol(m);
        s.kind == Kind::Def
            && (s.name == "||" || s.name == "&&")
            && s.owner.is_some_and(|o| l.u.full_name(o) == "scala.Boolean")
    })
}
