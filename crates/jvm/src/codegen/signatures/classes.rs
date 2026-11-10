// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Class signatures and type parameters with their bounds (dotty's `superSig`,
//! `minimizeParents`, `polyParamSig`, `boundsSig`).

use super::Sig;
use tasty::{SymbolId, TypeId, Universe};

/// The generic signature of class `c`: its type parameters and parents.
pub fn class(u: &mut Universe, c: SymbolId) -> String {
    let mut s = Sig::new(u, c);
    let params: Vec<(String, TypeId)> =
        (s.u.type_params(c).into_iter()).map(|p| (tasty::encode(&s.u.symbol(p).name), s.u.info(p))).collect();
    s.type_params(&params);
    let parents = s.u.parents(c);
    let mut leaves: Vec<(TypeId, SymbolId)> = Vec::new();
    for p in parents {
        let Some(ps) = s.u.class_symbol(p) else { continue };
        if leaves.iter().any(|&(_, l)| s.u.derives_from(l, ps)) {
            continue;
        }
        leaves.retain(|&(_, l)| !s.u.is_trait(l) || !s.u.derives_from(ps, l));
        leaves.push((p, ps));
    }
    if s.u.is_trait(c) {
        leaves.retain(|&(_, l)| s.u.is_trait(l));
    }
    if leaves.first().is_none_or(|&(_, l)| s.u.is_trait(l)) {
        s.out.push_str("Ljava/lang/Object;");
    }
    for (p, _) in leaves {
        s.boxed(p);
    }
    s.out
}

impl Sig<'_> {
    /// Type parameters by name and bounds, `<A:…;B::…;>`.
    pub fn type_params(&mut self, params: &[(String, TypeId)]) {
        if params.is_empty() {
            return;
        }
        self.out.push('<');
        for (name, bounds) in params {
            self.out.push_str(name);
            let hi = self.u.bounds(*bounds).map_or(*bounds, |(_, hi)| hi);
            let mut parts = Vec::new();
            self.and_parts(hi, &mut parts);
            let whole = self.u.erase(hi);
            let repr = parts.iter().position(|&p| self.u.erase(p) == whole).unwrap_or(0);
            self.out.push(':');
            if self.is_trait(parts[repr]) {
                self.out.push(':');
            }
            self.boxed(parts[repr]);
            for (i, &p) in parts.iter().enumerate() {
                if i != repr && self.is_trait(p) {
                    self.out.push(':');
                    self.boxed(p);
                }
            }
        }
        self.out.push('>');
    }
}
