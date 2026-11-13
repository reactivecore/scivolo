// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Generic signatures of classes, methods and fields (JVMS §4.7.9.1).

mod applied;
mod classes;
mod erased;
mod methods;
mod needed;
mod refs;
mod values;

pub use classes::class;
pub use erased::{descriptor_parts, erased_member};
pub use needed::erases_to;

use super::descriptors::descriptor;
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
    /// The trait member a mixin forwarder `sym0` forwards to, else `sym0`.
    origin: SymbolId,
    /// The value class of an extension method `sym0`, whose type parameters follow its own.
    extended: Option<SymbolId>,
    /// The top-level classes `sym0` is in, after and before local classes were lifted.
    tops: [SymbolId; 2],
    field: bool,
    constructor: bool,
    /// Whether the descriptor's result is `V`.
    void: bool,
    /// The method type parameters in scope: their binder, names and bounds.
    poly: Vec<(Binder, Vec<(String, TypeId)>)>,
    /// The method type parameters renamed, each by its name and its fresh name.
    renamed: Vec<(String, String)>,
    out: String,
}

/// A method or field a generic signature is made for.
#[derive(Clone, Copy)]
pub struct Member {
    pub sym: SymbolId,
    /// The trait member a mixin forwarder `sym` forwards to, else `sym`.
    pub origin: SymbolId,
    /// The value class of an extension method `sym`.
    pub extended: Option<SymbolId>,
    /// The top-level class `sym` is in, before local classes were lifted.
    pub top: SymbolId,
}

/// Builds the generic signature of `m` of type `info`, descriptor `desc` and thrown exception
/// types `thrown`; none if it equals `desc`.
pub fn member(u: &mut Universe, m: Member, info: TypeId, desc: &str, thrown: &[TypeId]) -> Option<String> {
    if !needed::may_need(u, info) {
        return None;
    }
    let method = desc.starts_with('(');
    let mut s = Sig::new(u, m.sym, m.top);
    s.origin = m.origin;
    s.extended = m.extended;
    s.field = !method;
    s.void = desc.ends_with(")V");
    match *s.u.ty(info) {
        Type::Method { .. } | Type::Poly { .. } => s.jsig(info, true, Boxing::Unbox),
        Type::ByName(_) => s.jsig(info, method, Boxing::Unbox),
        _ if method => {
            s.out.push_str("()");
            s.result(info);
        }
        _ => s.jsig(info, true, Boxing::Unbox),
    }
    for &t in thrown.iter().filter(|_| method) {
        s.out.push('^');
        s.jsig(t, true, Boxing::Unbox);
    }
    (!s.out.is_empty() && s.out != desc).then_some(s.out)
}

impl<'u> Sig<'u> {
    fn new(u: &'u mut Universe, sym0: SymbolId, original_top: SymbolId) -> Sig<'u> {
        let mut top = sym0;
        while let Some(o) = u.symbol(top).owner.filter(|&o| u.symbol(o).kind != Kind::Package) {
            top = o;
        }
        let constructor = u.is_constructor(sym0);
        let (origin, extended, poly, out) = (sym0, None, Vec::new(), String::new());
        let tops = [top, original_top];
        let (field, void, renamed) = (false, false, Vec::new());
        Sig { u, sym0, origin, extended, tops, field, constructor, void, poly, renamed, out }
    }

    fn boxed(&mut self, t: TypeId) {
        let t = self.u.widen(t);
        self.jsig(t, false, Boxing::Box);
    }

    fn and_parts(&mut self, t: TypeId, parts: &mut Vec<TypeId>) {
        let t = self.u.dealias(t);
        match *self.u.ty(t) {
            Type::And(a, b) => {
                self.and_parts(a, parts);
                self.and_parts(b, parts);
            }
            _ => parts.push(t),
        }
    }

    fn result(&mut self, t: TypeId) {
        if self.void { self.out.push('V') } else { self.jsig(t, false, Boxing::Unbox) }
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
        match *self.u.ty(t) {
            Type::Refined(p, ..) | Type::Annotated(p, _) | Type::Lambda { result: p, .. } => {
                self.jsig(p, top, boxing)
            }
            Type::Param(b, i) => match self.poly_param(b, i) {
                Some((_, bounds)) if self.primitive_bound(bounds) => self.erased(t, boxing),
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

    /// The type parameter `t` refers to, if it appears in the signature by name.
    fn type_param(&mut self, t: TypeId) -> Option<SymbolId> {
        let s = match *self.u.ty(t) {
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
            if self.tops.contains(&x) || x == self.sym0 || x == self.origin || Some(x) == self.extended {
                return Some(s);
            }
            o = self.u.symbol(x).owner;
        }
        None
    }
}
