// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/GenericSignatures.scala
//! Whether a member may need a generic signature, and whether one erases to a descriptor.

use tasty::{Kind, Type, TypeId, Universe};

/// Whether `t` names more than `Any`, `AnyRef`, `String` and primitive classes, a by-name
/// parameter `Function0` (dotty's `mayNeedSignature` after `ElimByName`).
pub fn may_need(u: &mut Universe, t: TypeId) -> bool {
    const PLAIN: [&str; 12] = [
        "scala.Any",
        "java.lang.Object",
        "java.lang.String",
        "scala.Unit",
        "scala.Boolean",
        "scala.Byte",
        "scala.Short",
        "scala.Char",
        "scala.Int",
        "scala.Long",
        "scala.Float",
        "scala.Double",
    ];
    let t = u.dealias(t);
    let class = match *u.ty(t) {
        Type::ByName(r) => return may_need(u, r),
        Type::Method { params, result, .. } => {
            return may_need(u, result)
                || (0..u.params_of(params).len()).any(|i| {
                    let p = u.params_of(params)[i].1;
                    matches!(u.ty(p), Type::ByName(_)) || may_need(u, p)
                });
        }
        Type::Sym(_, s) => Some(s),
        Type::TypeRef(..) => u.resolve_ref(t),
        _ => None,
    };
    let class = class.filter(|&c| u.symbol(c).kind == Kind::Class);
    class.is_none_or(|c| !PLAIN.contains(&u.full_name(c).as_str()))
}

/// Whether method or field signature `sig` erases to descriptor `desc`: a type variable of the
/// signature to the erasure of its first bound, another matching any type.
pub fn erases_to(sig: &str, desc: &str) -> bool {
    let (sig, desc) = (types(sig), types(desc));
    sig.len() == desc.len() && sig.iter().zip(&desc).all(|(s, d)| s == d || s.ends_with('?'))
}

/// The erased types of a signature in order, `?` for a type variable it does not declare.
fn types(sig: &str) -> Vec<String> {
    let b = sig.as_bytes();
    let mut params = Vec::new();
    let mut i = usize::from(b.first() == Some(&b'<'));
    while i > 0 && i < b.len() && b[i] != b'>' {
        let start = i;
        while b[i] != b':' {
            i += 1;
        }
        let name = sig[start..i].to_owned();
        let mut bound = None;
        while b.get(i) == Some(&b':') {
            i += 1;
            if b[i] != b':' {
                let e = erased(b, &mut i, &params);
                bound.get_or_insert(e);
            }
        }
        params.push((name, bound.unwrap_or_else(|| "?".into())));
    }
    i += usize::from(i > 0);
    let mut out = Vec::new();
    while i < b.len() {
        match b[i] {
            b'(' | b')' => i += 1,
            b'^' => break,
            _ => out.push(erased(b, &mut i, &params)),
        }
    }
    out
}

/// The erasure of the field type at `i` of signature `b`, past which `i` moves; `params` are the
/// declared type variables and their erasures.
fn erased(b: &[u8], i: &mut usize, params: &[(String, String)]) -> String {
    let c = b[*i];
    *i += 1;
    match c {
        b'[' => match (b[*i], erased(b, i, params)) {
            (b'T', e) if e == "Ljava/lang/Object;" || e.ends_with('?') => "?".into(),
            (_, e) => format!("[{e}"),
        },
        b'T' => {
            let start = *i;
            while b[*i] != b';' {
                *i += 1;
            }
            *i += 1;
            let name = std::str::from_utf8(&b[start..*i - 1]).unwrap_or_default();
            params.iter().rev().find(|(n, _)| n == name).map_or("?".into(), |(_, e)| e.clone())
        }
        b'L' => {
            let mut name = String::from("L");
            let mut depth = 0;
            while *i < b.len() && !(depth == 0 && b[*i] == b';') {
                match b[*i] {
                    b'<' => depth += 1,
                    b'>' => depth -= 1,
                    b'.' if depth == 0 => name.push('$'),
                    c if depth == 0 => name.push(c as char),
                    _ => {}
                }
                *i += 1;
            }
            *i += 1;
            name + ";"
        }
        c => (c as char).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::erases_to;

    #[test]
    fn erases_signatures() {
        assert!(erases_to("<T:Ljava/lang/Object;>([TT;)I", "(Ljava/lang/Object;)I"));
        assert!(erases_to(
            "()Lscala/Function1<Ljava/lang/Object;Ljava/lang/Object;>;",
            "()Lscala/Function1;"
        ));
        assert!(erases_to("()LBox<TA;>.Nested$;", "()LBox$Nested$;"));
        assert!(!erases_to(
            "<U:LElement;>(TU;TU;)TU;",
            "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;"
        ));
        assert!(erases_to("<U::Ljava/lang/Comparable<-TU;>;>(TU;)V", "(Ljava/lang/Comparable;)V"));
        assert!(!erases_to(
            "(Lscala/Product;)LSingle$;",
            "(Lscala/Product;)Lscala/deriving/Mirror$Singleton;"
        ));
    }
}
