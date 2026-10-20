// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/Definitions.scala
//! The specialized tuple classes `Tuple1$mc<T>$sp` and `Tuple2$mc<T1><T2>$sp` and the
//! specialized accessors `_1$mc<T>$sp` of `Tuple1` and `Tuple2` (dotty's `SpecializedTuple`,
//! `withSpecMethods`), defined if the library has no TASTy for them.

use super::{applied, member, package, symbol, type_ref};
use tasty::tags::FINAL;
use tasty::{Flags, Kind, SymbolId, Type, TypeId, Universe};

/// The element classes of the specialized `Tuple1` and `Tuple2` classes with their type tags
/// (dotty's `Tuple1SpecializedParamTypes`, `Tuple2SpecializedParamTypes`, `typeTag`).
const TUPLE1: [(&str, char); 3] = [("scala.Int", 'I'), ("scala.Long", 'J'), ("scala.Double", 'D')];
const TUPLE2: [(&str, char); 5] = [
    ("scala.Int", 'I'),
    ("scala.Long", 'J'),
    ("scala.Double", 'D'),
    ("scala.Char", 'C'),
    ("scala.Boolean", 'Z'),
];

/// The type tags of element classes `elems` if `Tuple{n}` of them is specialized, `n` their
/// number (dotty's `isSpecializableTuple`).
pub fn tuple_tags(elems: &[String]) -> Option<String> {
    let table: &[(&str, char)] = match elems.len() {
        1 => &TUPLE1,
        2 => &TUPLE2,
        _ => return None,
    };
    elems.iter().map(|e| table.iter().find(|(n, _)| n == e).map(|&(_, t)| t)).collect()
}

/// The primitive class type of type tag `tag`.
fn tag_type(u: &mut Universe, tag: char) -> TypeId {
    let name = TUPLE2.iter().find(|&&(_, t)| t == tag).map_or("scala.Int", |&(n, _)| n);
    u.class_ref(name)
}

/// The constructor of `Tuple{n}$mc<tags>$sp`, the class entered if missing as a final subclass
/// of `Tuple{n}` of the tags' primitive types.
pub fn specialized_tuple_ctor(u: &mut Universe, tags: &str) -> Result<SymbolId, String> {
    let elems: Vec<TypeId> = tags.chars().map(|t| tag_type(u, t)).collect();
    let scala = package(u, "scala")?;
    let base = member(u, scala, &format!("Tuple{}", elems.len()), Kind::Class)?;
    let name = format!("Tuple{}$mc{tags}$sp", elems.len());
    let class = match member(u, scala, &name, Kind::Class) {
        Ok(c) => c,
        Err(_) => {
            let c = u.define(symbol(&name, Kind::Class, scala, Flags::default().with(FINAL)));
            let parent = applied(u, base, elems.to_vec());
            u.set_parents(c, vec![parent]);
            c
        }
    };
    if let Ok(ctor) = member(u, class, "<init>", Kind::Def) {
        return Ok(ctor);
    }
    let ctor = u.define(symbol("<init>", Kind::Def, class, Flags::default()));
    let params = tags.chars().zip(elems).enumerate();
    let params = params.map(|(i, (t, e))| (format!("_{}$mc{t}$sp", i + 1), e)).collect();
    let result = type_ref(u, class);
    let binder = u.fresh_binder();
    let info = u.intern_type(Type::Method { binder, params, result, implicit: false, contextual: false });
    u.set_info(ctor, info);
    Ok(ctor)
}

/// Accessor `{field}$mc{tag}$sp` of `Tuple{n}`, of the tag's primitive type, entered if missing.
pub fn specialized_accessor(u: &mut Universe, n: usize, field: &str, tag: char) -> Result<SymbolId, String> {
    let elem = tag_type(u, tag);
    let scala = package(u, "scala")?;
    let base = member(u, scala, &format!("Tuple{n}"), Kind::Class)?;
    let name = format!("{field}$mc{tag}$sp");
    if let Ok(m) = member(u, base, &name, Kind::Def) {
        return Ok(m);
    }
    let m = u.define(symbol(&name, Kind::Def, base, Flags::default()));
    u.set_info(m, elem);
    Ok(m)
}
