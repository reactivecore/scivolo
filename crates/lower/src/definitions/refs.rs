//! The classes `scala.runtime.XRef` holding captured variables, defined if the library has no
//! TASTy for them.

use super::{java_class, java_static, member, package, type_ref};
use tasty::tags::MUTABLE;
use tasty::{Erased, Flags, Kind, SymbolId, Type, TypeId, Universe};

/// The class holding a captured variable, its field `elem` and its static method `create`.
#[derive(Clone, Copy)]
pub struct Ref {
    pub class: SymbolId,
    pub elem: SymbolId,
    pub create: SymbolId,
}

/// The class holding a captured variable of erased type `e` (dotty's `refClass`): `IntRef` for
/// `Int` and so on, `ObjectRef` for references.
pub fn ref_class(u: &mut Universe, e: &Erased) -> Result<Ref, String> {
    let (name, elem_class) = match e {
        Erased::Prim('z') => ("BooleanRef", "Boolean"),
        Erased::Prim('b') => ("ByteRef", "Byte"),
        Erased::Prim('c') => ("CharRef", "Char"),
        Erased::Prim('s') => ("ShortRef", "Short"),
        Erased::Prim('i') => ("IntRef", "Int"),
        Erased::Prim('j') => ("LongRef", "Long"),
        Erased::Prim('f') => ("FloatRef", "Float"),
        Erased::Prim('d') => ("DoubleRef", "Double"),
        _ => ("ObjectRef", ""),
    };
    let runtime = package(u, "scala.runtime")?;
    let class = java_class(u, runtime, name);
    let elem_type: TypeId = match elem_class {
        "" => type_ref(u, u.builtins().object),
        c => {
            let scala = package(u, "scala")?;
            let c = member(u, scala, c, Kind::Class)?;
            type_ref(u, c)
        }
    };
    let elem = match member(u, class, "elem", Kind::Val) {
        Ok(f) => f,
        Err(_) => {
            let f = u.define(super::symbol("elem", Kind::Val, class, Flags::default().with(MUTABLE)));
            u.set_info(f, elem_type);
            f
        }
    };
    let ref_type = type_ref(u, class);
    let binder = u.fresh_binder();
    let params = vec![("elem".to_owned(), elem_type)];
    let info =
        u.intern_type(Type::Method { binder, params, result: ref_type, implicit: false, contextual: false });
    let create = java_static(u, class, "create", Kind::Def, info);
    Ok(Ref { class, elem, create })
}
