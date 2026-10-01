//! Statement sequences: packages, blocks (dotty's `typedPackageDef`, `typedBlock`, `typedStats`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::{List, NodeId, Tree};
use tasty::{SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn typed_package_def(&mut self, id: NodeId, pid: NodeId, stats: List, ctx: Ctx) -> TNodeId {
        let Tree::Ident { name: pid_name } = *self.ast.tree(pid) else {
            return self.unsupported(pid, "qualified package name");
        };
        let simple = self.text(pid_name.name).to_owned();
        let name = match ctx.owner {
            o if o == self.u.root() => simple,
            o => format!("{}.{simple}", self.u.full_name(o)),
        };
        let pkg = self.u.enter_package(&name);
        let tpe = self.u.intern_type(Type::Package(pkg));
        let pid_span = self.ast.span(pid);
        let typed_pid = self.tpd.node(T::Ident { name: pid_name }, pid_span, Some(tpe), Some(pkg));
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
                out.push(self.typed(s, Pt::Any, ctx));
                ctx = self.after_stat(s, ctx);
            }
        }
        out
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
        let expr = match expr {
            Some(e) => self.typed(e, pt, bctx),
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
