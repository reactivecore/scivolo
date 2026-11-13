// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Class signatures and type parameters with their bounds (dotty's `superSig`,
//! `minimizeParents`, `polyParamSig`, `boundsSig`, `classSymbol`).

use super::Sig;
use tasty::{SymbolId, Type, TypeId, Universe};

/// Builds the generic signature of class `c` within top-level class `top` before local classes
/// were lifted: its type parameters and parents.
pub fn class(u: &mut Universe, c: SymbolId, top: SymbolId) -> String {
    let mut s = Sig::new(u, c, top);
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

/// How many lambdas, intersections and refined type members `Sig::class_of` follows.
const MAX_BOUNDS: u32 = 32;

impl Sig<'_> {
    /// Whether the class `t` stands for is a trait.
    pub fn is_trait(&mut self, t: TypeId) -> bool {
        self.class_of(t, MAX_BOUNDS).is_some_and(|c| self.u.is_trait(c))
    }

    /// The class `t` stands for (dotty's `classSymbol`): of a type lambda its result's, of an
    /// intersection the side deriving from the other, of a union its join's, of a type member of
    /// a refinement its bound's; `depth` bounds the members followed.
    fn class_of(&mut self, t: TypeId, depth: u32) -> Option<SymbolId> {
        let depth = depth.checked_sub(1)?;
        let t = self.u.dealias(t);
        let unresolved = self.u.resolve_ref(t).is_none();
        match *self.u.ty(t) {
            Type::Lambda { result, .. } | Type::Bounds(_, result) => self.class_of(result, depth),
            Type::And(a, b) => {
                let (a, b) = (self.class_of(a, depth)?, self.class_of(b, depth)?);
                match (self.u.derives_from(a, b), self.u.derives_from(b, a)) {
                    (true, _) => Some(a),
                    (_, true) => Some(b),
                    _ => None,
                }
            }
            Type::Or(..) => match self.u.join(t)? {
                j if j == t => None,
                j => self.class_of(j, depth),
            },
            Type::TypeRef(p, n) if unresolved => {
                let info = self.u.refinement_named(p, n)?;
                self.class_of(info, depth)
            }
            _ => self.u.class_symbol(t),
        }
    }

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
