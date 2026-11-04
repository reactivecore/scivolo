use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::Kind;

impl Typer<'_> {
    /// Makes the types of all trees, and of closures' symbols, fully defined; `Err` if a local
    /// symbol escaped into the bound of a type variable.
    pub fn zonk_trees(&mut self) -> Result<(), &'static str> {
        for s in self.syms.closure_syms.clone() {
            let info = self.u.info(s);
            let info = self.fully_defined(info)?;
            self.u.set_info(s, info);
        }
        for i in 0..self.tpd.len() {
            let id = TNodeId::new(i);
            if let Some(t) = self.tpd.get(id).tpe {
                let t = self.fully_defined(t)?;
                self.tpd.get_mut(id).tpe = Some(t);
            }
            let defines = matches!(self.tpd.tree(id), T::ValDef { .. } | T::DefDef { .. } | T::Bind { .. });
            if let Some(s) = self.tpd.sym(id).filter(|_| defines)
                && !self.u.symbol(s).flags.has(tasty::tags::INLINEPROXY)
                && !(self.syms.closure_syms.contains(&s)
                    && matches!(self.u.symbol(s).kind, Kind::Param | Kind::Def))
            {
                let info = self.u.info(s);
                if super::deskolemize::has_skolem(self.u, info) {
                    let bind = matches!(self.tpd.tree(id), T::Bind { .. });
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
}
