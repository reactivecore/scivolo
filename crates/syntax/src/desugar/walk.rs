// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//! The traversal: each tree desugared as the typer would meet it, then its children.

use super::{Desugar, Mode, Owner};
use crate::ast::tree::Clauses;
use crate::ast::{Attrs, List, NodeId, Tree};

impl Desugar<'_> {
    /// The desugared tree `id` (the oracle's `Expander.transform`).
    pub fn transform(&mut self, id: NodeId) -> NodeId {
        match self.ast.tree(id).clone() {
            Tree::ForYield { .. }
            | Tree::ForDo { .. }
            | Tree::InterpolatedString { .. }
            | Tree::SymbolLit { .. }
            | Tree::PostfixOp { .. }
            | Tree::PrefixOp { .. } => {
                let t = self.desugar_expr(id);
                if t == id { self.map_children(id) } else { self.transform(t) }
            }
            Tree::Parens { t } => {
                let t = self.transform(t);
                for check in [Attrs::CHECK_PAT_DEF, Attrs::CHECK_GEN_FROM] {
                    if self.ast.attrs(id).contains(check) {
                        self.ast.add_attrs(t, check);
                    }
                }
                t
            }
            Tree::InfixOp { left, op, right } => {
                let args = self.list(&[left, right]);
                let t = match self.mode {
                    Mode::Type => self.ast.derived(id, Tree::AppliedTypeTree { tpt: op, args }),
                    Mode::Pattern => self.ast.derived(id, Tree::Apply { fun: op, args }),
                    Mode::Expr => {
                        let t = self.binop(left, op, right);
                        self.ast.add_attrs(t, self.ast.attrs(id));
                        t
                    }
                };
                self.transform(t)
            }
            Tree::Tuple { trees } if !self.has_named_arg(trees) => {
                let t = self.tuple(id, trees);
                self.ast.add_attrs(t, self.ast.attrs(id));
                if t == id { self.map_children(id) } else { self.transform(t) }
            }
            Tree::Block { .. } => {
                let b = self.block(id);
                self.transform_block(b)
            }
            Tree::Typed { expr, tpt } => {
                let expr = self.transform(expr);
                let tpt = self.in_mode(Mode::Type, |d| d.transform(tpt));
                self.ast.derived(id, Tree::Typed { expr, tpt })
            }
            Tree::New { tpt } => {
                let tpt = if matches!(self.ast.tree(tpt), Tree::Template { .. }) {
                    let owner = Owner::Class { is_trait: false, is_package_object: false };
                    let full_name = self.child_full_name("$anon");
                    let class = self.new_class(full_name, Vec::new(), false, None);
                    self.in_class(class, owner, |d| d.transform_template(tpt))
                } else {
                    self.in_mode(Mode::Type, |d| d.transform(tpt))
                };
                self.ast.derived(id, Tree::New { tpt })
            }
            Tree::TypeApply { fun, args } => {
                let fun = self.transform(fun);
                let args = self.in_mode(Mode::Type, |d| d.transform_list(args));
                self.ast.derived(id, Tree::TypeApply { fun, args })
            }
            Tree::Annotated { arg, annot } => {
                let arg = self.transform(arg);
                let annot = self.in_mode(Mode::Expr, |d| d.transform(annot));
                self.ast.derived(id, Tree::Annotated { arg, annot })
            }
            Tree::SingletonTypeTree { .. } => self.in_mode(Mode::Expr, |d| d.map_children(id)),
            Tree::AppliedTypeTree { .. }
            | Tree::LambdaTypeTree { .. }
            | Tree::TypeBoundsTree { .. }
            | Tree::ByNameTypeTree { .. }
            | Tree::MatchTypeTree { .. }
            | Tree::RefinedTypeTree { .. }
            | Tree::ContextBounds { .. }
            | Tree::ContextBoundTypeTree { .. }
            | Tree::TermLambdaTypeTree { .. } => self.in_mode(Mode::Type, |d| d.map_children(id)),
            Tree::CaseDef { pat, guard, body } => {
                let pat_mode = if self.mode == Mode::Type { Mode::Type } else { Mode::Pattern };
                let pat = self.in_mode(pat_mode, |d| d.transform(pat));
                let (guard, body) = self.in_mode(Mode::Expr, |d| (d.transform_opt(guard), d.transform(body)));
                self.ast.derived(id, Tree::CaseDef { pat, guard, body })
            }
            Tree::GenFrom { pat, expr, check_mode } => {
                let pat = self.in_mode(Mode::Pattern, |d| d.transform(pat));
                let expr = self.transform(expr);
                self.ast.derived(id, Tree::GenFrom { pat, expr, check_mode })
            }
            Tree::GenAlias { pat, expr } => {
                let pat = self.in_mode(Mode::Pattern, |d| d.transform(pat));
                let expr = self.transform(expr);
                self.ast.derived(id, Tree::GenAlias { pat, expr })
            }
            Tree::ValDef { .. } | Tree::DefDef { .. } | Tree::TypeDef { .. } | Tree::ModuleDef { .. } => {
                self.transform_def(id)
            }
            Tree::Import { .. } | Tree::Export { .. } => self.transform_import(id),
            Tree::Template { .. } => self.transform_template(id),
            Tree::Quote { body } => {
                let mode = if self.ast.is_type(body) { Mode::Type } else { Mode::Expr };
                let body = self.in_mode(mode, |d| d.transform(body));
                self.ast.derived(id, Tree::Quote { body })
            }
            Tree::SplicePattern { body } => {
                let body = self.in_mode(Mode::Pattern, |d| d.transform(body));
                self.ast.derived(id, Tree::SplicePattern { body })
            }
            _ => self.map_children(id),
        }
    }

    pub fn transform_opt(&mut self, t: Option<NodeId>) -> Option<NodeId> {
        t.map(|t| self.transform(t))
    }

    /// The transformed trees of `l`, flattened; `l` itself if none changed.
    pub fn transform_list(&mut self, l: List) -> List {
        let items = self.ast.items(l).to_vec();
        let mapped: Vec<NodeId> = items.iter().map(|&t| self.transform(t)).collect();
        let flat = self.flatten(&mapped);
        if flat == items { l } else { self.list(&flat) }
    }

    pub fn transform_clauses(&mut self, c: Clauses) -> Clauses {
        let clauses = self.ast.clause_items(c).to_vec();
        let mapped: Vec<List> = clauses.iter().map(|&l| self.transform_list(l)).collect();
        if mapped == clauses { c } else { self.ast.clauses(&mapped) }
    }

    fn has_named_arg(&self, trees: List) -> bool {
        self.ast.items(trees).iter().any(|&t| matches!(self.ast.tree(t), Tree::NamedArg { .. }))
    }
}
