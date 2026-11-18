// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Whether a member may need a generic signature.

use tasty::{Kind, PRIMITIVE_VALUE_CLASSES, Type, TypeId, Universe};

/// Whether `t` names more than `Any`, `AnyRef`, `String` and primitive classes, also as the upper
/// bound of a type parameter or member (dotty's `mayNeedSignature`); a by-name parameter is a
/// `Function0`. With `aliased`, as dotty checks a static forwarder after erasure, an alias of other
/// than a primitive class, such as `AnyRef` or `Predef.String`, names more.
pub fn may_need(u: &mut Universe, t: TypeId, aliased: bool) -> bool {
    const PLAIN: [&str; 3] = ["scala.Any", "java.lang.Object", "java.lang.String"];
    let primitive = |u: &Universe, c| PRIMITIVE_VALUE_CLASSES.contains(&u.full_name(c).as_str());
    let declared = t;
    let t = u.dealias_opaque(t);
    let class = match *u.ty(t) {
        Type::ByName(_) => return true,
        Type::Method { params, result, .. } => {
            return may_need(u, result, aliased)
                || (0..u.params_of(params).len()).any(|i| {
                    let p = u.params_of(params)[i].1;
                    matches!(u.ty(p), Type::ByName(_)) || may_need(u, p, aliased)
                });
        }
        _ if aliased && t != declared => return u.class_symbol(t).is_none_or(|c| !primitive(u, c)),
        Type::Sym(_, s) => Some(s),
        Type::TypeRef(..) => u.resolve_ref(t),
        _ => None,
    };
    let class = class.and_then(|s| match u.symbol(s).kind {
        Kind::Class => Some(s),
        _ => u.class_symbol(t).filter(|&c| primitive(u, c)),
    });
    class.is_none_or(|c| !PLAIN.contains(&u.full_name(c).as_str()) && !primitive(u, c))
}
