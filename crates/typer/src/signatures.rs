// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Signature.scala
//   compiler/src/dotty/tools/dotc/core/TypeErasure.scala
//! Methods' signatures as dotty's `Signature.paramsSig` gives them: the number of type parameters
//! of each type parameter clause and the erased class of each value parameter.

use crate::defn::type_ref;
use crate::tpd::print::symbols::full_name;
use tasty::{Binder, Erased, SymbolId, Type, TypeId, Universe};

/// The parameter part of the signature of method `s`, e.g. `["1", "java.lang.Object"]`.
pub fn params_sig(u: &mut Universe, s: SymbolId) -> Vec<String> {
    let mut t = u.info(s);
    let mut parts = Vec::new();
    let mut bounds: Vec<(Binder, Vec<TypeId>)> = Vec::new();
    loop {
        match *u.ty(t) {
            Type::Poly { binder, params, result } => {
                let params = u.params_of(params);
                parts.push(params.len().to_string());
                let his = params.iter().map(|&(_, b)| upper_bound(u, b)).collect();
                bounds.push((binder, his));
                t = result;
            }
            Type::Method { params, result, .. } => {
                for i in 0..u.params_of(params).len() {
                    let p = u.params_of(params)[i].1;
                    if u.is_erased_param(p) {
                        continue;
                    }
                    let p = if u.param_ref(p).is_some() {
                        bounded(u, p, &bounds)
                    } else {
                        u.map_type(p, &mut |u, x| {
                            u.param_ref(x)?;
                            let hi = bounded(u, x, &bounds);
                            Some(u.intern_type(Type::Bounds(tasty::NOTHING, hi)))
                        })
                    };
                    let e = match u.erase(p) {
                        Erased::Unit => Erased::Class("scala.runtime.BoxedUnit".into()),
                        e => e,
                    };
                    parts.push(sig_name(u, p, &e));
                }
                t = result;
            }
            _ => return parts,
        }
    }
}

/// The result part of the signature of method `s`, a constructor's its class or a value class's
/// underlying type, `None` if `s` is not a method (dotty's `Signature.NotAMethod`).
pub fn result_sig(u: &mut Universe, s: SymbolId) -> Option<String> {
    let mut t = u.info(s);
    let mut method = false;
    while let Some(b) = u.poly(t).or_else(|| u.method(t)) {
        (t, method) = (b.result, true);
    }
    if !method {
        return None;
    }
    if u.is_constructor(s) {
        let c = u.symbol(s).owner?;
        if u.underlying(c).is_none() {
            return Some(full_name(u, c));
        }
        t = type_ref(u, c);
    }
    Some(match u.erase(t) {
        Erased::Unit => "scala.Unit".into(),
        e => sig_name(u, t, &e),
    })
}

/// The name of an erased parameter type in a signature: `scala.Int`, `java.lang.String[]`.
fn sig_name(u: &mut Universe, t: TypeId, e: &Erased) -> String {
    match e {
        Erased::Prim('R') => "scala.scalanative.runtime.RawPtr".into(),
        Erased::Prim('w') => "scala.scalanative.runtime.RawSize".into(),
        Erased::Prim(c) => format!("scala.{}", prim_name(*c)),
        Erased::Unit => "scala.runtime.BoxedUnit".into(),
        Erased::Nothing => "scala.Nothing".into(),
        Erased::Null => "scala.Null".into(),
        Erased::Array(elem) => format!("{}[]", sig_name(u, t, elem)),
        Erased::Value(_, underlying) => sig_name(u, t, underlying),
        Erased::Class(binary) => {
            let found = u.erased_class(t).or_else(|| {
                let widened = u.widen(t);
                u.erased_class(widened).or_else(|| alias_class(u, widened)).or_else(|| {
                    let r = u.class_ref(binary);
                    u.class_symbol(r)
                })
            });
            match found.filter(|&c| u.binary_name(c) == *binary).or_else(|| class_in(u, t, binary, 8)) {
                Some(c) => full_name(u, c),
                None => binary.clone(),
            }
        }
    }
}

/// The class of the right-hand side of the type alias `t` applies, through its type lambda.
fn alias_class(u: &mut Universe, t: TypeId) -> Option<SymbolId> {
    let (tycon, _) = u.applied(t)?;
    let d = u.dealias(tycon);
    let mut result = u.lambda(d)?.result;
    while let Type::Refined(parent, ..) = *u.ty(result) {
        result = parent;
    }
    u.class_symbol(result)
}

/// The class of binary name `binary` that type `t` mentions, or a base class of one, through
/// aliases and opaque types.
fn class_in(u: &mut Universe, t: TypeId, binary: &str, depth: u32) -> Option<SymbolId> {
    let depth = depth.checked_sub(1)?;
    let mut syms = Vec::new();
    u.map_type(t, &mut |u, x| {
        if let Some(s) = u.sym_of(x) {
            syms.push(s);
        }
        None
    });
    for s in syms {
        if u.is_class(s) {
            let base = u.linearization(s).into_iter().find(|&b| u.binary_name(b) == binary);
            if base.is_some() {
                return base;
            }
        }
        if u.is_type_symbol(s) && !u.is_class(s) {
            let info = u.opaque_alias(s).unwrap_or_else(|| u.info(s));
            if let Some(c) = class_in(u, info, binary, depth) {
                return Some(c);
            }
        }
    }
    None
}

fn prim_name(c: char) -> &'static str {
    match c {
        'z' => "Boolean",
        'c' => "Char",
        'b' => "Byte",
        's' => "Short",
        'i' => "Int",
        'j' => "Long",
        'f' => "Float",
        'd' => "Double",
        _ => "?",
    }
}

fn upper_bound(u: &Universe, b: TypeId) -> TypeId {
    u.bounds(b).map_or(b, |(_, hi)| hi)
}

/// `t` with references to the type parameters of polymorphic types `bounds` replaced by their
/// upper bounds, as erasure sees them.
fn bounded(u: &mut Universe, t: TypeId, bounds: &[(Binder, Vec<TypeId>)]) -> TypeId {
    u.map_type(t, &mut |u, x| {
        let (b, i) = u.param_ref(x)?;
        let his = &bounds.iter().find(|(binder, _)| *binder == b)?.1;
        let hi = *his.get(i as usize)?;
        Some(bounded(u, hi, &[]))
    })
}

/// The binders of the method types of method `s`'s type if `terms`, else of its poly types,
/// outermost first.
pub fn method_binders(u: &mut Universe, s: SymbolId, terms: bool) -> Vec<Binder> {
    let mut info = u.info(s);
    let mut binders = Vec::new();
    loop {
        let (b, method) = match (u.poly(info), u.method(info)) {
            (Some(b), _) => (b, false),
            (_, Some(b)) => (b, true),
            _ => return binders,
        };
        if method == terms {
            binders.push(b.binder);
        }
        info = b.result;
    }
}
