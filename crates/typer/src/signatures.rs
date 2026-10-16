//! Methods' signatures as dotty's `Signature.paramsSig` gives them: the number of type parameters
//! of each type parameter clause and the erased class of each value parameter.

use crate::tpd::print::symbols::full_name;
use tasty::{Binder, Erased, SymbolId, Type, TypeId, Universe};

/// The parameter part of the signature of method `s`, e.g. `["1", "java.lang.Object"]`.
pub fn params_sig(u: &mut Universe, s: SymbolId) -> Vec<String> {
    let mut t = u.info(s);
    let mut parts = Vec::new();
    let mut bounds: Vec<(Binder, Vec<TypeId>)> = Vec::new();
    loop {
        match u.types().get(t).clone() {
            Type::Poly { binder, params, result } => {
                parts.push(params.len().to_string());
                let his = params.iter().map(|&(_, b)| upper_bound(u, b)).collect();
                bounds.push((binder, his));
                t = result;
            }
            Type::Method { params, result, .. } => {
                for (_, p) in params {
                    let p = match u.types().get(p) {
                        Type::Param(..) => bounded(u, p, &bounds),
                        _ => u.map_type(p, &mut |u, x| match *u.types().get(x) {
                            Type::Param(..) => {
                                let hi = bounded(u, x, &bounds);
                                Some(u.intern_type(Type::Bounds(tasty::NOTHING, hi)))
                            }
                            _ => None,
                        }),
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

/// The name of an erased parameter type in a signature: `scala.Int`, `java.lang.String[]`.
fn sig_name(u: &mut Universe, t: TypeId, e: &Erased) -> String {
    match e {
        Erased::Prim(c) => format!("scala.{}", prim_name(*c)),
        Erased::Unit => "scala.runtime.BoxedUnit".into(),
        Erased::Nothing => "scala.Nothing".into(),
        Erased::Null => "scala.Null".into(),
        Erased::Array(elem) => format!("{}[]", sig_name(u, t, elem)),
        Erased::Value(_, underlying) => sig_name(u, t, underlying),
        Erased::Class(binary) => match u.erased_class(t).or_else(|| {
            let widened = u.widen(t);
            u.erased_class(widened).or_else(|| alias_class(u, widened)).or_else(|| {
                let r = u.class_ref(binary);
                u.class_symbol(r)
            })
        }) {
            Some(c) if u.binary_name(c) == *binary => full_name(u, c),
            _ => binary.clone(),
        },
    }
}

/// The class of the right-hand side of the type alias `t` applies, through its type lambda.
fn alias_class(u: &mut Universe, t: TypeId) -> Option<SymbolId> {
    let Type::Applied(tycon, _) = *u.types().get(t) else { return None };
    let d = u.dealias(tycon);
    let Type::Lambda { mut result, .. } = *u.types().get(d) else { return None };
    while let Type::Refined(parent, ..) = *u.types().get(result) {
        result = parent;
    }
    u.class_symbol(result)
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
    match u.types().get(b) {
        Type::Bounds(_, hi) => *hi,
        _ => b,
    }
}

/// `t` with references to the type parameters of polymorphic types `bounds` replaced by their
/// upper bounds, as erasure sees them.
fn bounded(u: &mut Universe, t: TypeId, bounds: &[(Binder, Vec<TypeId>)]) -> TypeId {
    u.map_type(t, &mut |u, x| match *u.types().get(x) {
        Type::Param(b, i) => {
            let his = &bounds.iter().find(|(binder, _)| *binder == b)?.1;
            let hi = *his.get(i as usize)?;
            Some(bounded(u, hi, &[]))
        }
        _ => None,
    })
}

/// The binders of the method types of method `s`'s type, outermost first.
pub fn method_binders(u: &mut Universe, s: SymbolId) -> Vec<Binder> {
    let mut info = u.info(s);
    let mut binders = Vec::new();
    loop {
        match u.types().get(info).clone() {
            Type::Poly { result, .. } => info = result,
            Type::Method { binder, result, .. } => {
                binders.push(binder);
                info = result;
            }
            _ => return binders,
        }
    }
}
