// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenStat.scala
//! nscplugin's `genInterfaceMethodBridgeForDefDef`: a method overriding an abstract member, not
//! overloaded, of a trait of the unit or the library its class directly extends, with another
//! erased signature it conforms to, gets a bridge of that member's type calling it, itself a
//! bridge if the method is; at the method, its body at the class if it takes parameters.

use super::Gen;
use crate::Result;
use tasty::{Erased, Kind, SymbolId};
use typer::tpd::{TNodeId, Template, Tree};

impl Gen<'_, '_> {
    /// The bridges method definition `dd` needs to the abstract members of the traits its class
    /// directly extends.
    pub fn interface_bridges(&mut self, dd: TNodeId) -> Result<Vec<TNodeId>> {
        let Some(m) = self.l.tpd.sym(dd) else { return Ok(Vec::new()) };
        let class = self.class.expect("a class");
        if self.l.u.is_constructor(m) {
            return Ok(Vec::new());
        }
        let name = self.l.u.symbol(m).name.clone();
        let (params, result) = self.l.u.erased_method(m);
        let bases = self.l.u.linearization(class);
        let before_superclass = bases[1..].iter().copied().take_while(|&b| self.l.u.is_trait(b));
        let mut traits: Vec<SymbolId> = before_superclass.collect();
        traits.reverse();
        let mut seen: Vec<(Vec<Erased>, Erased)> = Vec::new();
        let mut out = Vec::new();
        for t in traits {
            let decls = self.l.u.decl(t, &name).unwrap_or_default();
            let terms = decls.iter().filter(|&&d| self.l.u.symbol(d).kind.is_term()).count();
            for inherited in decls.into_iter().filter(|_| terms == 1) {
                let library = self.l.u.symbol(inherited).tree.is_some();
                let deferred =
                    self.deferred.contains(&inherited) || library && self.l.u.body(inherited).is_none();
                if !matches!(self.l.u.symbol(inherited).kind, Kind::Def | Kind::Val) || !deferred {
                    continue;
                }
                let signature = self.l.u.erased_method(inherited);
                if signature.0 != params || signature.1 == result || seen.contains(&signature) {
                    continue;
                }
                let bottom = matches!(result, Erased::Nothing | Erased::Null);
                let to_reference = matches!(signature.1, Erased::Class(_) | Erased::Array(_));
                let reference = matches!(result, Erased::Class(_) | Erased::Array(_));
                let conforms = reference && crate::phases::casts::conforms(self.l, &result, &signature.1);
                if !(conforms || bottom && to_reference) {
                    continue;
                }
                seen.push(signature);
                let at = self.spans.get(&m).copied().unwrap_or(self.l.tpd.span(dd));
                let span = match params.is_empty() {
                    true => at,
                    false => self.spans.get(&class).copied().unwrap_or(at),
                };
                let info = self.l.u.info(inherited);
                let m_info = self.l.u.info(m);
                let m_result = self.l.u.result_type(m_info);
                let flags = self.l.u.symbol(m).flags;
                let (bridge, def) =
                    self.l.method_like(class, inherited, flags, info, span, |l, args, _| {
                        let this = l.this(class, span);
                        let fun = l.select(this, m, span);
                        l.apply(fun, args, m_result, span)
                    })?;
                self.l.u.unenter(bridge);
                self.l.tpd.get_mut(def).span = at;
                if self.l.bridges.contains(&m) {
                    self.l.bridges.insert(bridge);
                }
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
        let Some(Template { body, .. }) = tpd.class_template(td) else { continue };
        for &stat in tpd.items(body) {
            if let (Tree::DefDef { rhs: None, .. }, Some(s)) = (tpd.tree(stat), tpd.sym(stat)) {
                out.insert(s);
            }
        }
    }
    out
}
