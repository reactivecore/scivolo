// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! Applications, ascriptions, lambdas and blocks of `Printer.TreeCode`.

use super::{Code, REFUSED, Shown};
use syntax::ast::constant::Constant;
use tasty::Type;
use typer::tpd::{TNodeId, Tree as T};

impl Code<'_> {
    pub fn apply(&mut self, fun: TNodeId, args: &[TNodeId]) -> Shown {
        let callee = self.h.tpd.sym(self.r(fun));
        let special = callee
            .and_then(|s| self.h.u.symbol(s).owner)
            .is_some_and(|o| self.h.u.symbol(o).name == "<special-ops>");
        if special && callee.is_some_and(|s| self.h.u.symbol(s).name == "throw") && args.len() == 1 {
            self.push("throw ");
            return self.tree(args[0]);
        }
        match self.tree_of(fun) {
            T::Select { qualifier, name }
                if self.name(name) == "<init>" && matches!(self.tree_of(qualifier), T::This { .. }) =>
            {
                self.push("this")
            }
            _ => self.qual_tree(fun)?,
        }
        let args = match args.split_last() {
            Some((&last, init)) if self.is_empty_varargs(last) => init,
            _ => args,
        };
        self.push("(");
        self.trees(args, ", ")?;
        self.push(")");
        Ok(())
    }

    fn is_empty_varargs(&self, t: TNodeId) -> bool {
        let T::Typed { expr, .. } = self.tree_of(t) else { return false };
        matches!(self.tree_of(expr), T::SeqLiteral { elems, .. } if elems.is_empty())
    }

    /// Whether `t` is of type `<repeated>[T]`.
    fn is_repeated(&mut self, t: TNodeId) -> bool {
        let Some(tpe) = self.h.tpd.get(t).tpe else { return false };
        let Type::Applied(c, _) = *self.h.u.types().get(tpe) else { return false };
        self.h.u.class_symbol(c).is_some_and(|c| self.h.u.full_name(c) == "scala.<repeated>")
    }

    pub fn typed(&mut self, expr: TNodeId, tpt: TNodeId) -> Shown {
        let tpt = self.r(tpt);
        if self.is_repeated(tpt) {
            self.tree(expr)?;
            let seq = match self.tree_of(expr) {
                T::SeqLiteral { .. } => true,
                T::Inlined { call: None, bindings, expansion } => {
                    bindings.is_empty() && matches!(self.tree_of(expansion), T::SeqLiteral { .. })
                }
                _ => false,
            };
            if !seq {
                self.push(": _*");
            }
            return Ok(());
        }
        self.push("(");
        self.tree(expr)?;
        let operator = self.out.ends_with(|c: char| "~!@#%^*+-<>?:=&|/\\".contains(c));
        self.push(if operator { " : " } else { ": " });
        let tpe = self.h.tpd.tpe(tpt);
        if matches!(self.h.u.types().get(tpe), Type::Annotated(..)) {
            return Err(REFUSED);
        }
        self.ty(tpe)?;
        self.push(")");
        Ok(())
    }

    /// The parameters and body of `t` if it is a lambda: a block of a method definition and its
    /// closure (dotty's `Lambda` extractor).
    pub fn lambda(&self, t: TNodeId) -> Option<(Vec<TNodeId>, TNodeId)> {
        let T::Block { stats, expr } = self.tree_of(t) else { return None };
        let [def] = self.items(stats)[..] else { return None };
        let T::DefDef { paramss, rhs: Some(body), .. } = self.tree_of(def) else { return None };
        let T::Closure { meth, .. } = self.tree_of(expr) else { return None };
        if self.h.tpd.sym(self.r(meth)) != self.h.tpd.sym(def) || self.h.tpd.sym(def).is_none() {
            return None;
        }
        let [params] = self.h.tpd.clause_items(paramss)[..] else { return None };
        Some((self.items(params), body))
    }

    pub fn print_lambda(&mut self, t: TNodeId) -> Shown {
        let (params, body) = self.lambda(t).ok_or(REFUSED)?;
        let tpe = self.h.tpd.tpe(t);
        let context = self
            .h
            .u
            .class_symbol(tpe)
            .is_some_and(|c| self.h.u.full_name(c).starts_with("scala.ContextFunction"));
        self.push("(");
        self.lambda_params(&params)?;
        self.push(if context { " ?=> " } else { " => " });
        self.tree(body)?;
        self.push(")");
        Ok(())
    }

    /// The statements and expression of `stats` and `expr` with nested blocks and inlined
    /// trees flattened and unit literals dropped (dotty's `flatBlock`).
    fn flatten(&self, stats: &[TNodeId], expr: TNodeId) -> (Vec<TNodeId>, TNodeId) {
        let mut flat = Vec::new();
        for &s in stats {
            self.flat_stat(s, &mut flat);
        }
        let mut expr = self.r(expr);
        loop {
            match self.tree_of(expr) {
                _ if self.lambda(expr).is_some() => break,
                T::Block { stats, expr: e } | T::Inlined { bindings: stats, expansion: e, .. } => {
                    for s in self.items(stats) {
                        self.flat_stat(s, &mut flat);
                    }
                    expr = self.r(e);
                }
                _ => break,
            }
        }
        (flat, expr)
    }

    fn flat_stat(&self, s: TNodeId, flat: &mut Vec<TNodeId>) {
        let s = self.r(s);
        match self.tree_of(s) {
            _ if self.lambda(s).is_some() => flat.push(s),
            T::Block { stats, expr: e } | T::Inlined { bindings: stats, expansion: e, .. } => {
                for x in self.items(stats) {
                    self.flat_stat(x, flat);
                }
                self.flat_stat(e, flat);
            }
            T::Literal { value: Constant::Unit } => {}
            _ => flat.push(s),
        }
    }

    pub fn flat_block(&mut self, stats: &[TNodeId], expr: TNodeId) -> Shown {
        let (stats, expr) = self.flatten(stats, expr);
        if stats.is_empty() {
            return self.tree(expr);
        }
        self.push("{");
        self.indent += 1;
        self.stats(&stats, expr)?;
        self.indent -= 1;
        self.line_break();
        self.push("}");
        Ok(())
    }

    pub fn stats(&mut self, stats: &[TNodeId], expr: TNodeId) -> Shown {
        self.line_break();
        for (i, &s) in stats.iter().enumerate() {
            self.tree(s)?;
            self.separator(stats.get(i + 1).copied().unwrap_or(expr));
        }
        self.tree(expr)
    }

    /// The break before statement `next`, double where a `{` could apply to the line before.
    fn separator(&mut self, next: TNodeId) {
        let next = self.r(next);
        if matches!(
            self.tree_of(next),
            T::ValDef { .. } | T::DefDef { .. } | T::TypeDef { .. } | T::Import { .. }
        ) {
            return self.line_break();
        }
        let (stats, expr) = self.flatten(&[], next);
        let mut t = stats.first().copied().unwrap_or(expr);
        let double = loop {
            match self.tree_of(t) {
                _ if self.lambda(t).is_some() => break false,
                T::Block { stats, .. } => break !stats.is_empty(),
                T::Inlined { bindings, .. } => break !bindings.is_empty(),
                T::Select { qualifier: f, .. } | T::Apply { fun: f, .. } | T::TypeApply { fun: f, .. } => {
                    t = self.r(f)
                }
                T::Typed { .. } => break true,
                _ => break false,
            }
        };
        if double {
            self.out.push('\n');
        }
        self.line_break();
    }
}
