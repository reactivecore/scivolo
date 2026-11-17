use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::Span;
use tasty::{Kind, SymbolId};

impl Typer<'_> {
    /// Makes the types of all trees, and of closures' and signature polymorphic instances'
    /// symbols, fully defined; `Err` if a local symbol escaped into the bound of a type variable.
    pub fn zonk_trees(&mut self) -> Result<(), &'static str> {
        let spans = self.closure_spans();
        for s in self.syms.closure_syms.clone() {
            if let Some(&span) = spans.get(&s) {
                self.keep_captures_at(span);
            }
            let info = self.u.info(s);
            let info = self.fully_defined(info)?;
            self.u.set_info(s, info);
        }
        for i in 0..self.tpd.len() {
            let id = TNodeId::new(i);
            self.keep_captures_at(self.tpd.span(id));
            if let Some(t) = self.tpd.get(id).tpe {
                self.fully_defined(t)?;
                let t = self.types().zonk_kept(t);
                self.tpd.get_mut(id).tpe = Some(t);
            }
            if let Some(s) = self.tpd.sym(id).filter(|&s| self.u.original_signature_polymorphic(s).is_some())
            {
                let info = self.u.info(s);
                let info = self.fully_defined(info)?;
                self.u.set_info(s, info);
            }
            let defines = matches!(self.tpd.tree(id), T::ValDef { .. } | T::DefDef { .. } | T::Bind { .. });
            if let Some(s) = self.tpd.sym(id).filter(|_| defines)
                && !self.u.symbol(s).flags.has(tasty::tags::INLINEPROXY)
                && !(self.syms.closure_syms.contains(&s)
                    && matches!(self.u.symbol(s).kind, Kind::Param | Kind::Def))
            {
                let bind = matches!(self.tpd.tree(id), T::Bind { .. });
                let mut info = self.u.info(s);
                if bind {
                    info = self.fully_defined(info)?;
                    self.u.set_info(s, info);
                }
                if super::deskolemize::has_skolem(self.u, info) {
                    match super::deskolemize::skolems_widened(self.u, info).filter(|_| bind) {
                        Some(info) => self.u.set_info(s, info),
                        None => return Err("skolem in the type of a definition"),
                    }
                }
            }
        }
        if self.constraint.escaped() {
            return Err("local type escaping through a type variable");
        }
        Ok(())
    }

    /// The span of the definition of each closure symbol, in a unit of several files.
    fn closure_spans(&self) -> FxHashMap<SymbolId, Span> {
        if self.file_cfgs.len() < 2 {
            return FxHashMap::default();
        }
        let closures: FxHashSet<SymbolId> = self.syms.closure_syms.iter().copied().collect();
        (0..self.tpd.len())
            .map(TNodeId::new)
            .filter(|&id| matches!(self.tpd.tree(id), T::ValDef { .. } | T::DefDef { .. }))
            .filter_map(|id| self.tpd.sym(id).map(|s| (s, self.tpd.span(id))))
            .filter(|(s, _)| closures.contains(s))
            .collect()
    }
}
