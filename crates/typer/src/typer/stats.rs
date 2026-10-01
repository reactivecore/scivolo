//! Statement sequences: packages, blocks (dotty's `typedPackageDef`, `typedBlock`, `typedStats`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, Tree};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn typed_package_def(&mut self, id: NodeId, pid: NodeId, stats: List, ctx: Ctx) -> TNodeId {
        let Some(typed_pid) = self.typed_pid(pid, ctx.owner) else {
            return self.unsupported(pid, "package name");
        };
        let pkg = self.tpd.sym(typed_pid).expect("a package");
        let tpe = self.tpd.tpe(typed_pid);
        let level = self.levels.push(Some(ctx.level), Level::Members(pkg));
        let pctx = Ctx { owner: pkg, level, ..ctx };
        let stat_ids = self.ast.items(stats).to_vec();
        self.index(&stat_ids, pctx);
        let mut typed = self.typed_stats(&stat_ids, pctx);
        let proxies = self.main_proxies(&typed, pctx);
        typed.extend(proxies);
        let stats = self.tpd.list(&typed);
        let span = self.ast.span(id);
        self.tpd.node(T::PackageDef { pid: typed_pid, stats }, span, Some(tpe), None)
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

    /// Whether type `t` refers to one of `syms`.
    fn refers_to(&mut self, t: TypeId, syms: &[SymbolId]) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            if let Type::Sym(_, s) = u.types().get(part)
                && syms.contains(s)
            {
                found = true;
            }
            None
        });
        found
    }

    /// The typed statements `stats`, whose symbols are entered.
    pub fn typed_stats(&mut self, stats: &[NodeId], mut ctx: Ctx) -> Vec<TNodeId> {
        let mut out = Vec::new();
        for &s in stats {
            if !matches!(self.ast.tree(s), Tree::Empty) {
                out.push(self.typed_stat(s, ctx));
                ctx = self.after_stat(s, ctx);
            }
        }
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
        if !definition {
            self.check_statement_purity(typed);
        }
        typed
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
        let stat_ids = self.ast.items(stats).to_vec();
        self.index(&stat_ids, bctx);
        let typed = self.typed_stats(&stat_ids, bctx);
        let expr_ctx = stat_ids.iter().fold(bctx, |c, &s| self.after_stat(s, c));
        let expr = match expr {
            Some(e) => self.typed(e, pt, expr_ctx),
            None => return self.unsupported(id, "block without result"),
        };
        let tpe = self.tpd.tpe(expr);
        let locals = self.levels.scope_symbols(level);
        if self.refers_to(tpe, &locals) {
            return self.unsupported(id, "block type with local symbols");
        }
        let stats = self.tpd.list(&typed);
        let span = self.ast.span(id);
        self.tpd.node(T::Block { stats, expr }, span, Some(tpe), None)
    }
}
