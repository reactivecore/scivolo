//! nscplugin's `genInterfaceMethodBridgeForDefDef`: a method overriding an abstract member of a
//! trait its class directly extends, with another erased signature, gets a bridge of that
//! member's type calling it.

use super::Gen;
use crate::Result;
use crate::symbols::{is_constructor, is_trait};
use tasty::{Erased, Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// The bridges method definition `dd` needs to the abstract members of the traits its class
    /// directly extends.
    pub fn interface_bridges(&mut self, dd: TNodeId) -> Result<Vec<TNodeId>> {
        let Some(m) = self.l.tpd.sym(dd) else { return Ok(Vec::new()) };
        let class = self.class.expect("a class");
        if is_constructor(self.l.u, m) {
            return Ok(Vec::new());
        }
        let name = self.l.u.symbol(m).name.clone();
        let (params, result) = self.l.u.erased_method(m);
        let parents = self.l.u.parents(class);
        let classes: Vec<SymbolId> = parents.into_iter().filter_map(|p| self.l.u.class_symbol(p)).collect();
        let traits: Vec<SymbolId> = classes.into_iter().filter(|&t| is_trait(self.l.u, t)).collect();
        let mut seen: Vec<(Vec<Erased>, Erased)> = Vec::new();
        let mut out = Vec::new();
        for t in traits {
            for inherited in self.l.u.decl(t, &name).unwrap_or_default() {
                if !matches!(self.l.u.symbol(inherited).kind, Kind::Def | Kind::Val)
                    || !self.deferred.contains(&inherited)
                {
                    continue;
                }
                let signature = self.l.u.erased_method(inherited);
                if signature.0 != params || signature.1 == result || seen.contains(&signature) {
                    continue;
                }
                seen.push(signature);
                let span = self.l.tpd.span(dd);
                let info = self.l.u.info(inherited);
                let m_info = self.l.u.info(m);
                let m_result = self.l.u.result_type(m_info);
                let flags = self.l.u.symbol(m).flags;
                let (_, def) = self.l.method_like(class, inherited, flags, info, span, |l, args, _| {
                    let this = l.this(class, span);
                    let fun = l.select(this, m, span);
                    l.apply(fun, args, m_result, span)
                })?;
                out.push(def);
            }
        }
        Ok(out)
    }
}

/// The abstract methods of the unit's classes: those defined without a body.
pub fn deferred_methods(tpd: &typer::tpd::Tpd, type_defs: &[TNodeId]) -> rustc_hash::FxHashSet<SymbolId> {
    let mut out = rustc_hash::FxHashSet::default();
    for &td in type_defs {
        let Tree::TypeDef { rhs, .. } = *tpd.tree(td) else { continue };
        let Tree::Template { body, .. } = *tpd.tree(rhs) else { continue };
        for &stat in tpd.items(body) {
            if let (Tree::DefDef { rhs: None, .. }, Some(s)) = (tpd.tree(stat), tpd.sym(stat)) {
                out.insert(s);
            }
        }
    }
    out
}
