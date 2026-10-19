// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymUtils.scala
//   compiler/src/dotty/tools/dotc/transform/Bridges.scala
// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nscplugin/src/main/scala-3/scala/scalanative/nscplugin/NirGenStat.scala
//! Bridge methods: dotty's (`Bridges`), in classes, for a concrete method overriding a base
//! class method whose erased signature differs; Scala Native's (`genInterfaceMethodBridge`), in
//! classes and traits, for a method implementing the only method of its name in a trait it
//! inherits before its superclass, abstract there with another signature.

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
        let name = self.symbol(def).name.clone();
        let params = self.erased_method(def).0;
        for t in self.inherited_traits(c) {
            let terms: Vec<SymbolId> = self
                .decl(t, &name)
                .unwrap_or_default()
                .into_iter()
                .filter(|&m| !self.is_type_symbol(m))
                .collect();
            let [other] = terms[..] else { continue };
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

    /// The traits among the base classes of `c` before its superclass (dotty's
    /// `directlyInheritedTraits`).
    fn inherited_traits(&mut self, c: SymbolId) -> Vec<SymbolId> {
        let parents: Vec<SymbolId> =
            self.parents(c).into_iter().filter_map(|p| self.class_symbol(p)).collect();
        let superclass = parents.into_iter().find(|&p| !self.symbol(p).flags.has(TRAIT));
        let bases = self.linearization(c).into_iter().skip(1);
        bases.take_while(|&b| Some(b) != superclass).filter(|&b| self.symbol(b).flags.has(TRAIT)).collect()
    }

    /// Whether the erased result type of method `a` conforms to that of method `b`.
    fn erased_result_conforms(&mut self, a: SymbolId, b: SymbolId) -> bool {
        let (ia, ib) = (self.info(a), self.info(b));
        let (ra, rb) = (self.result_type(ia), self.result_type(ib));
        let (ea, eb) = (self.erase(ra), self.erase(rb));
        match (&ea, &eb) {
            (Erased::Class(_) | Erased::Array(_), Erased::Class(o)) if o == OBJECT => true,
            (Erased::Array(x), Erased::Array(y)) => {
                x == y || matches!(**y, Erased::Class(ref o) if o == OBJECT)
            }
            (Erased::Class(_), Erased::Class(_)) => match (self.erased_class(ra), self.erased_class(rb)) {
                (Some(x), Some(y)) => self.derives_from(x, y),
                _ => false,
            },
            _ => ea == eb,
        }
    }

    /// Whether method `def` has a right-hand side; true for a method without a tree.
    fn has_body(&self, def: SymbolId) -> bool {
        let Some((file, id)) = self.symbol(def).tree else { return true };
        let (ast, _) = self.file(file);
        matches!(view(ast, id), View::DefDef { parts, .. } if parts.body.len() > 1)
    }
}
