// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Generic signatures of classes, methods and fields (JVMS §4.7.9.1).

mod classes;
mod erasure;
mod refs;

pub use classes::class;

use super::descriptor;
use tasty::{Binder, Erased, Kind, SymbolId, Type, TypeId, Universe};

#[derive(Clone, Copy, PartialEq)]
enum Boxing {
    Box,
    Unbox,
    UnboxOnlyPrimitives,
}

struct Sig<'u> {
    u: &'u mut Universe,
    sym0: SymbolId,
    top: Option<SymbolId>,
    field: bool,
    constructor: bool,
    /// The method type parameters in scope: their binder, names and bounds.
    poly: Vec<(Binder, Vec<(String, TypeId)>)>,
    out: String,
}

/// The generic signature of method or field `sym` of type `info` and descriptor `desc`; none if
/// it equals `desc`.
pub fn member(u: &mut Universe, sym: SymbolId, info: TypeId, desc: &str) -> Option<String> {
    let method = desc.starts_with('(');
    let mut s = Sig::new(u, sym);
    s.field = !method;
    match s.u.types().get(info) {
        Type::Method { .. } | Type::Poly { .. } | Type::ByName(_) => s.jsig(info, true, Boxing::Unbox),
        _ if method => {
            s.out.push_str("()");
            s.result(info);
        }
        _ => s.jsig(info, true, Boxing::Unbox),
    }
    let erases = s.constructor || erasure::erases_to(&s.out, desc);
    (!s.out.is_empty() && s.out != desc && erases).then_some(s.out)
}

impl<'u> Sig<'u> {
    fn new(u: &'u mut Universe, sym0: SymbolId) -> Sig<'u> {
        let mut top = Some(sym0);
        while let Some(o) = top.and_then(|t| u.symbol(t).owner).filter(|&o| u.symbol(o).kind != Kind::Package)
        {
            top = Some(o);
        }
        let constructor = u.is_constructor(sym0);
        Sig { u, sym0, top, field: false, constructor, poly: Vec::new(), out: String::new() }
    }

    fn boxed(&mut self, t: TypeId) {
        let t = self.u.widen(t);
        self.jsig(t, false, Boxing::Box);
    }

    fn and_parts(&mut self, t: TypeId, parts: &mut Vec<TypeId>) {
        let t = self.u.dealias(t);
        match *self.u.types().get(t) {
            Type::And(a, b) => {
                self.and_parts(a, parts);
                self.and_parts(b, parts);
            }
            _ => parts.push(t),
        }
    }

    fn is_trait(&mut self, t: TypeId) -> bool {
        self.u.class_symbol(t).is_some_and(|c| self.u.is_trait(c))
    }

    fn result(&mut self, t: TypeId) {
        let unit = self.class_named(t, "scala.Unit");
        if unit || self.constructor { self.out.push('V') } else { self.jsig(t, false, Boxing::Unbox) }
    }

    fn class_named(&mut self, t: TypeId, name: &str) -> bool {
        let t = self.u.dealias(t);
        matches!(self.u.types().get(t), Type::Sym(..) | Type::TypeRef(..))
            && self.u.class_symbol(t).is_some_and(|c| self.u.full_name(c) == name)
    }

    fn erased(&mut self, t: TypeId, boxing: Boxing) {
        let e = self.u.erase(t);
        self.erased_sig(&e, boxing);
    }

    fn erased_sig(&mut self, e: &Erased, boxing: Boxing) {
        match e {
            Erased::Unit => self.out.push_str("Lscala/runtime/BoxedUnit;"),
            Erased::Prim(_) if boxing == Boxing::Box => self.out.push_str("Ljava/lang/Object;"),
            Erased::Prim(_) if self.out.is_empty() && self.field => {}
            Erased::Value(_, u) if boxing != Boxing::Box => self.erased_sig(&u.clone(), boxing),
            e => self.out.push_str(&descriptor(e)),
        }
    }

    fn jsig(&mut self, t: TypeId, top: bool, boxing: Boxing) {
        let t = self.u.dealias(t);
        match self.u.types().get(t).clone() {
            Type::Refined(p, ..) | Type::Annotated(p, _) | Type::Lambda { result: p, .. } => {
                self.jsig(p, top, boxing)
            }
            Type::Param(b, i) => match self.poly_param(b, i) {
                Some((name, _)) => self.out.push_str(&format!("T{name};")),
                None => self.erased(t, boxing),
            },
            Type::Sym(_, s) if self.u.symbol(s).kind.is_term() => self.widened(t, top, boxing),
            Type::TermRef(..) | Type::This(_) | Type::Constant(..) | Type::Str(_) => {
                self.widened(t, top, boxing)
            }
            Type::ByName(r) if top => {
                self.out.push_str("()");
                self.result(r);
            }
            Type::ByName(r) => self.class_sig("scala/Function0", &[r]),
            Type::Method { .. } | Type::Poly { .. } => self.method(t, top),
            Type::Or(a, b, _) => match (self.class_named(a, "scala.Null"), self.class_named(b, "scala.Null"))
            {
                (false, true) if !self.is_value(a) => self.jsig(a, top, boxing),
                (true, false) if !self.is_value(b) => self.jsig(b, top, boxing),
                _ => self.erased(t, boxing),
            },
            Type::And(..) => {
                let mut parts = Vec::new();
                self.and_parts(t, &mut parts);
                let whole = self.u.erase(t);
                let mut reprs = parts.into_iter().filter(|&p| self.u.erase(p) == whole).collect::<Vec<_>>();
                let param = reprs.iter().position(|&p| self.type_param(p).is_some()).unwrap_or(0);
                match reprs.is_empty() {
                    true => self.erased(t, boxing),
                    false => self.jsig(reprs.swap_remove(param), false, boxing),
                }
            }
            Type::Sym(..) | Type::TypeRef(..) | Type::Applied(..) => self.reference(t, boxing),
            _ => self.erased(t, boxing),
        }
    }

    fn widened(&mut self, t: TypeId, top: bool, boxing: Boxing) {
        match self.u.widen_once(t) {
            Some(w) => self.jsig(w, top, boxing),
            None => self.erased(t, boxing),
        }
    }

    fn is_value(&mut self, t: TypeId) -> bool {
        let any_val = self.u.builtins().any_val;
        self.u.class_symbol(t).is_some_and(|c| any_val.is_some_and(|v| self.u.derives_from(c, v)))
    }

    /// The type parameter `t` refers to, if it appears in the signature by name.
    fn type_param(&mut self, t: TypeId) -> Option<SymbolId> {
        let s = match *self.u.types().get(t) {
            Type::Sym(_, s) => s,
            Type::TypeRef(..) => self.u.resolve_ref(t)?,
            _ => return None,
        };
        let owner = self.u.symbol(s).owner?;
        if self.u.symbol(s).kind != Kind::TypeParam
            || self.u.symbol(owner).kind.is_type() && !self.u.is_class(owner)
        {
            return None;
        }
        let mut o = Some(s);
        while let Some(x) = o {
            if Some(x) == self.top || x == self.sym0 {
                return Some(s);
            }
            o = self.u.symbol(x).owner;
        }
        None
    }
}
