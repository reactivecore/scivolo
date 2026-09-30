//! Bridge methods: dotty's (`Bridges`), in classes, for a concrete method overriding a base
//! class method whose erased signature differs; Scala Native's (`genInterfaceMethodBridge`), in
//! classes and traits, for a method implementing the only method of its name in a directly
//! inherited trait, abstract there with another signature.

use super::{Erased, OBJECT};
use crate::tags::{PRIVATE, TRAIT};
use crate::types::Type;
use crate::{Kind, SymbolId, Universe, View, view};

impl Universe {
    /// The signatures of the bridges the owner of method `def` declares for it.
    pub fn bridge_signatures(&mut self, def: SymbolId) -> Vec<String> {
        let Some(c) = self.symbol(def).owner else { return Vec::new() };
        if self.symbol(c).kind != Kind::Class {
            return Vec::new();
        }
        let Some(own) = self.method_signature(def) else { return Vec::new() };
        let mut out = Vec::new();
        if !self.symbol(c).flags.has(TRAIT) && self.has_body(def) {
            self.dotty_bridges(c, def, &own, &mut out);
        }
        self.interface_bridges(c, def, &own, &mut out);
        out
    }

    fn dotty_bridges(&mut self, c: SymbolId, def: SymbolId, own: &str, out: &mut Vec<String>) {
        let site = self.intern_type(Type::Sym(None, c));
        let site = self.intern_type(Type::This(site));
        let name = self.symbol(def).name.clone();
        for base in self.linearization(c).into_iter().skip(1) {
            for other in self.decl(base, &name).unwrap_or_default() {
                if self.symbol(other).kind != Kind::Def || self.symbol(other).flags.has(PRIVATE) {
                    continue;
                }
                let Some(sig) = self.method_signature(other) else { continue };
                if sig != own && !out.contains(&sig) && self.matches(site, def, other) {
                    out.push(sig);
                }
            }
        }
    }

    fn interface_bridges(&mut self, c: SymbolId, def: SymbolId, own: &str, out: &mut Vec<String>) {
        if !self.has_body(def) {
            return;
        }
        let name = self.symbol(def).name.clone();
        let params = self.erased_method(def).0;
        for parent in self.parents(c) {
            let Some(t) = self.class_symbol(parent).filter(|&t| self.symbol(t).flags.has(TRAIT)) else {
                continue;
            };
            let [other] = self.decl(t, &name).unwrap_or_default()[..] else { continue };
            if self.symbol(other).kind != Kind::Def || self.has_body(other) {
                continue;
            }
            let Some(sig) = self.method_signature(other) else { continue };
            if sig != own
                && !out.contains(&sig)
                && self.erased_method(other).0 == params
                && self.erased_result_conforms(def, other)
            {
                out.push(sig);
            }
        }
    }

    /// Whether the erased result type of method `a` is a reference type deriving from that of
    /// method `b`.
    fn erased_result_conforms(&mut self, a: SymbolId, b: SymbolId) -> bool {
        let (ia, ib) = (self.info(a), self.info(b));
        let (ra, rb) = (self.result_type(ia), self.result_type(ib));
        match (self.erase(ra), self.erase(rb)) {
            (Erased::Class(_) | Erased::Array(_), Erased::Class(o)) if o == OBJECT => true,
            (Erased::Class(_), Erased::Class(_)) => match (self.erased_class(ra), self.erased_class(rb)) {
                (Some(x), Some(y)) => self.derives_from(x, y),
                _ => false,
            },
            (x, y) => x == y,
        }
    }

    /// Whether method `def` has a right-hand side.
    fn has_body(&self, def: SymbolId) -> bool {
        let Some((file, id)) = self.symbol(def).tree else { return false };
        let (ast, _) = self.file(file);
        matches!(view(ast, id), View::DefDef { parts, .. } if parts.body.len() > 1)
    }
}
