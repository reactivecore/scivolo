// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Class signatures and type parameters with their bounds (dotty's `superSig`,
//! `minimizeParents`, `polyParamSig`, `boundsSig`, `classSymbol`).

use super::Sig;
use tasty::{Erased, Kind, SymbolId, Type, TypeId, Universe};

/// The full name of class `Null`.
pub const NULL: &str = "scala.Null";

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
    /// intersection the side deriving from the other, of a union `union_class`, of a refinement its
    /// parent's, of a type member of a refinement, an abstract type or a type parameter, also
    /// applied, its bound's; `depth` bounds the members followed.
    fn class_of(&mut self, t: TypeId, depth: u32) -> Option<SymbolId> {
        let depth = depth.checked_sub(1)?;
        let t = self.u.dealias_opaque(t);
        let unresolved = self.u.resolve_ref(t).is_none();
        let ty = *self.u.ty(t);
        match ty {
            Type::Lambda { result, .. }
            | Type::Bounds(_, result)
            | Type::Refined(result, _, _)
            | Type::Rec(_, result) => self.class_of(result, depth),
            Type::And(a, b) => {
                let (a, b) = (self.class_of(a, depth)?, self.class_of(b, depth)?);
                match (self.u.derives_from(a, b), self.u.derives_from(b, a)) {
                    (true, _) => Some(a),
                    (_, true) => Some(b),
                    _ => None,
                }
            }
            Type::Or(a, b, _) => self.union_class(t, (a, b), depth),
            Type::Param(b, i) => {
                let (_, bounds) = self.poly_param(b, i)?;
                self.class_of(bounds, depth)
            }
            Type::Applied(tycon, _) if self.u.param_ref(tycon).is_some() => match self.upper_bounded(t) {
                applied if applied == t => self.class_of(tycon, depth),
                applied => self.class_of(applied, depth),
            },
            Type::TypeRef(p, n) if unresolved => {
                let info = self.u.refinement_named(p, n)?;
                self.class_of(info, depth)
            }
            Type::Sym(..) | Type::TypeRef(..) if let Some(hi) = self.u.abstract_upper(t) => {
                self.class_of(hi, depth)
            }
            _ => self.u.class_symbol(t),
        }
    }

    /// The class of union `t` of `sides` (dotty's `join`): with `Null`, the other side's class if
    /// `Null` conforms to it, else `Matchable` or `Any`; else its join's.
    fn union_class(&mut self, t: TypeId, (a, b): (TypeId, TypeId), depth: u32) -> Option<SymbolId> {
        if self.class_named(a, NULL) || self.class_named(b, NULL) {
            let other = if self.class_named(a, NULL) { b } else { a };
            let c = self.class_of(other, depth)?;
            let d = self.u.defn();
            let value = d.any_val.is_some_and(|v| self.u.derives_from(c, v));
            if !self.u.null_mode().explicit && !value {
                return Some(c);
            }
            return Some(if self.u.derives_from(c, d.matchable) { d.matchable } else { d.any });
        }
        match self.u.join(t)? {
            j if j == t => None,
            j => self.class_of(j, depth),
        }
    }

    /// Whether `t` refers to the class of full name `name`.
    pub fn class_named(&mut self, t: TypeId, name: &str) -> bool {
        self.class_ref(t).is_some_and(|c| self.u.full_name(c) == name)
    }

    /// The class `t` refers to after dealiasing, none for a type parameter or member.
    pub fn class_ref(&mut self, t: TypeId) -> Option<SymbolId> {
        let t = self.u.dealias_opaque(t);
        let s = match *self.u.ty(t) {
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(..) => self.u.resolve_ref(t),
            _ => None,
        };
        s.filter(|&s| self.u.symbol(s).kind == Kind::Class)
    }

    /// The erasure of the first of intersection `parts` in dotty's `erasedGlb`.
    pub fn glb_erasure(&mut self, parts: &[TypeId]) -> Erased {
        let glb = self.u.erased_glb(parts).expect("an intersection's parts");
        self.u.erase(glb)
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
            let bounded: Vec<TypeId> = parts.iter().map(|&p| self.upper_bounded(p)).collect();
            let whole = self.glb_erasure(&bounded);
            let repr = bounded.iter().position(|&p| self.u.erase(p) == whole).unwrap_or(0);
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
