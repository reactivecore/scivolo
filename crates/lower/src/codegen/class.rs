//! Classes, modules and traits with their methods (nscplugin's `genClass` and `genMethods`).

use super::Gen;
use crate::symbols::is_static_module;
use crate::{Result, unsupported};
use nir::{Attr, Defn, DefnKind};
use tasty::tags::ABSTRACT;
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    pub fn gen_class(&mut self, td: TNodeId) -> Result<()> {
        let sym = self.l.tpd.sym(td).expect("a class definition's symbol");
        self.class = Some(sym);
        if crate::symbols::is_extern(self.l.u, sym) {
            return unsupported(self.l.tpd.span(td), "native interop");
        }
        let pos = self.pos_or(self.l.tpd.span(td), self.no_pos);
        let attrs =
            if crate::symbols::has(self.l.u, sym, ABSTRACT) { vec![Attr::Abstract] } else { Vec::new() };
        let name = self.type_name(sym);
        let traits = self.class_interfaces(sym);
        let kind = if is_static_module(self.l.u, sym) {
            DefnKind::Module { parent: Some(self.class_parent(sym)), traits }
        } else if self.l.u.is_trait(sym) {
            DefnKind::Trait { traits }
        } else {
            DefnKind::Class { parent: Some(self.class_parent(sym)), traits }
        };
        self.defns.push(Defn { name, attrs, pos, kind });
        if self.l.u.is_module_class(sym) {
            self.gen_lazy_handle_fields(sym, pos);
        }
        self.gen_lazy_handle_accessors(sym, pos);
        let Tree::TypeDef { rhs: template, .. } = *self.l.tpd.tree(td) else {
            unreachable!("a class definition")
        };
        self.gen_class_fields(sym, template, pos);
        self.lambdas = 0;
        self.gen_methods(td)?;
        let closures = std::mem::take(&mut self.closures);
        self.defns.extend(closures);
        self.gen_mirror_class(sym, pos)
    }

    /// The superclass of `c`, `java.lang.Object` for none.
    fn class_parent(&mut self, c: tasty::SymbolId) -> nir::GlobalId {
        let parents = self.l.u.parents(c);
        let superclass = parents
            .first()
            .and_then(|&p| self.l.u.class_symbol(p))
            .filter(|&s| !self.l.u.is_trait(s))
            .unwrap_or(self.l.defs.object);
        self.type_name(superclass)
    }

    fn class_interfaces(&mut self, c: tasty::SymbolId) -> Vec<nir::GlobalId> {
        let parents = self.l.u.parents(c);
        let classes: Vec<_> = parents.into_iter().filter_map(|p| self.l.u.class_symbol(p)).collect();
        let traits: Vec<_> = classes.into_iter().filter(|&s| self.l.u.is_trait(s)).collect();
        traits.into_iter().map(|t| self.type_name(t)).collect()
    }

    fn gen_methods(&mut self, td: TNodeId) -> Result<()> {
        let Tree::TypeDef { rhs, .. } = *self.l.tpd.tree(td) else { unreachable!("a class definition") };
        let Tree::Template { constr, body, .. } = *self.l.tpd.tree(rhs) else {
            return unsupported(self.l.tpd.span(td), "a type definition that is not a class");
        };
        let stats: Vec<TNodeId> =
            std::iter::once(constr).chain(self.l.tpd.items(body).iter().copied()).collect();
        let first = self.defns.len();
        for stat in stats {
            match self.l.tpd.tree(stat) {
                Tree::DefDef { .. } => {
                    let defn = self.gen_method(stat)?;
                    self.defns.push(defn);
                    for bridge in self.interface_bridges(stat)? {
                        let defn = self.gen_method(bridge)?;
                        self.defns.push(defn);
                    }
                }
                Tree::TypeDef { .. } => {}
                Tree::ValDef { .. } => {}
                _ => return unsupported(self.l.tpd.span(stat), "statements in a class body"),
            }
        }
        let existing: Vec<nir::GlobalId> = self.defns[first..].iter().map(|d| d.name).collect();
        let forwarders = self.static_method_forwarders(&existing)?;
        self.defns.extend(forwarders);
        Ok(())
    }
}
