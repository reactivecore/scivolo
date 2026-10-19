//! Statement sequences: packages, blocks (dotty's `typedPackageDef`, `typedBlock`, `typedStats`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, Tree};
use tasty::{SymbolId, Type};

impl Typer<'_> {
    pub fn typed_package_def(&mut self, id: NodeId, pid: NodeId, stats: List, ctx: Ctx) -> TNodeId {
        let Some(typed_pid) = self.typed_pid(pid, ctx.owner) else {
            return self.unsupported(pid, "package name");
        };
        let pkg = self.tpd.sym(typed_pid).expect("a package");
        let tpe = self.tpd.tpe(typed_pid);
        let stat_ids = self.ast.items(stats).to_vec();
        let pctx = match self.package_levels.get(&id) {
            Some(&level) => Ctx { owner: pkg, level, ..ctx },
            None => {
                let level = self.levels.push(Some(ctx.level), Level::Members(pkg));
                let pctx = Ctx { owner: pkg, level, ..ctx };
                self.index(&stat_ids, pctx);
                pctx
            }
        };
        let mut typed = self.typed_stats(&stat_ids, pctx);
        let proxies = self.main_proxies(&typed, pctx);
        typed.extend(proxies);
        let stats = self.tpd.list(&typed);
        let span = self.ast.span(id);
        if pkg != self.u.root() {
            self.check_package_object_name(&stat_ids, span);
        }
        self.tpd.node(T::PackageDef { pid: typed_pid, stats }, span, Some(tpe), None)
    }

    /// Warns at `span` if the class path encodes the name of the package object among `stats`
    /// that declares something not synthetic.
    fn check_package_object_name(&mut self, stats: &[NodeId], span: syntax::ast::Span) {
        let objects = stats.iter().filter_map(|s| self.syms.by_tree.get(s).copied());
        let Some(c) =
            objects.into_iter().find(|&c| self.u.is_class(c) && self.u.symbol(c).name.ends_with("$package$"))
        else {
            return;
        };
        let declares = self.u.symbol(c).decls.iter().any(|&d| {
            self.u.symbol(d).name != "<init>"
                && !self.syms.flags.get(&d).is_some_and(|fl| fl.is(syntax::ast::flags::SYNTHETIC))
        });
        let name = self.u.symbol(c).name.trim_end_matches('$').to_owned();
        if declares && syntax::desugar::package_def::will_be_encoded(&name) {
            let msg = syntax::desugar::package_def::encoded_package_name(&name);
            self.diags.warning(span.point(), msg);
        }
    }

    /// The package that package clause `pid`, `p` or `p.q`, inside package `owner` names, entered
    /// with the packages on its path; `_root_` is the root.
    pub fn typed_pid(&mut self, pid: NodeId, owner: SymbolId) -> Option<TNodeId> {
        let (outer, name) = match *self.ast.tree(pid) {
            Tree::Ident { name } => (None, name),
            Tree::Select { qualifier, name } => (Some(self.typed_pid(qualifier, owner)?), name),
            _ => return None,
        };
        let text = self.text(name.name).to_owned();
        let prefix = match outer {
            Some(q) => self.tpd.sym(q)?,
            None => owner,
        };
        let pkg = if outer.is_none() && text == "_root_" {
            self.u.root()
        } else if prefix == self.u.root() {
            self.u.enter_package(&text)
        } else {
            let full = format!("{}.{text}", self.u.full_name(prefix));
            self.u.enter_package(&full)
        };
        let tpe = self.u.intern_type(Type::Package(pkg));
        let span = self.ast.span(pid);
        let tree = match outer {
            Some(qualifier) => T::Select { qualifier, name },
            None => T::Ident { name },
        };
        Some(self.tpd.node(tree, span, Some(tpe), Some(pkg)))
    }

    /// The typed statements `stats`, whose symbols are entered.
    pub fn typed_stats(&mut self, stats: &[NodeId], mut ctx: Ctx) -> Vec<TNodeId> {
        let mut out = Vec::new();
        let mut ctxs = Vec::new();
        for &s in stats {
            if !matches!(self.ast.tree(s), Tree::Empty) {
                out.push(self.typed_stat(s, ctx));
                ctxs.push(ctx);
                ctx = self.after_stat(s, ctx);
            }
        }
        self.finalize_derived(&out, &ctxs);
        out
    }

    /// Statement `s`, an expression checked for purity.
    pub fn typed_stat(&mut self, s: NodeId, ctx: Ctx) -> TNodeId {
        let typed = self.typed(s, Pt::Any, ctx);
        let definition = matches!(
            self.ast.tree(s),
            Tree::ValDef { .. }
                | Tree::DefDef { .. }
                | Tree::TypeDef { .. }
                | Tree::Import { .. }
                | Tree::PackageDef { .. }
        );
        if !definition && !matches!(self.ast.tree(s), Tree::Function { .. }) && self.is_eta_closure(typed) {
            return self.unsupported(s, "method reference in statement position");
        }
        if !definition && !self.warn_on_interesting_result(typed) {
            self.check_statement_purity(typed);
        }
        typed
    }

    /// Whether `s` is the setter desugaring gives a variable, which a block does not define
    /// (dotty's `isSetterNeeded` for a local owner).
    fn is_setter(&self, s: NodeId) -> bool {
        matches!(*self.ast.tree(s), Tree::DefDef { mods, .. }
            if self.ast.modifiers(mods).flags.is_all_of(syntax::ast::flags::ACCESSOR.or(syntax::ast::flags::MUTABLE)))
    }

    pub fn typed_block(
        &mut self,
        id: NodeId,
        stats: List,
        expr: Option<NodeId>,
        pt: Pt,
        ctx: Ctx,
    ) -> TNodeId {
        let level = self.levels.push(Some(ctx.level), Level::Scope(Vec::new()));
        let bctx = Ctx { level, ..ctx };
        let stat_ids: Vec<NodeId> =
            self.ast.items(stats).iter().copied().filter(|&s| !self.is_setter(s)).collect();
        self.index(&stat_ids, bctx);
        let typed = self.typed_stats(&stat_ids, bctx);
        let expr_ctx = stat_ids.iter().fold(bctx, |c, &s| self.after_stat(s, c));
        let expr = match expr {
            Some(e) => self.typed(e, pt, expr_ctx),
            None => return self.unsupported(id, "block without result"),
        };
        let locals = self.levels.scope_symbols(level);
        let Some(expr) = self.no_local_refs(expr, &locals, &[], pt) else {
            return self.unsupported(id, "block type with local symbols");
        };
        let stats = self.tpd.list(&typed);
        let span = self.ast.span(id);
        self.build().block(stats, expr, span)
    }
}
