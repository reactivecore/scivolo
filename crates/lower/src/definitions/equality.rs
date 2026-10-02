use super::{java_class, java_static, member, package, type_ref};
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

/// What `==` on references and `##` call, and the classes whose instances may be boxed values.
#[derive(Clone, Copy)]
pub struct Equality {
    pub any_equals: SymbolId,
    pub boxes_runtime: SymbolId,
    /// `BoxesRunTime.equals(Object, Object)`, a Java static method.
    pub equals: SymbolId,
    pub statics: SymbolId,
    /// `Statics.anyHash(Object)`, a Java static method.
    pub any_hash: SymbolId,
    pub number: SymbolId,
    pub character: SymbolId,
    pub boolean: SymbolId,
    pub serializable: SymbolId,
    pub comparable: SymbolId,
}

impl Equality {
    pub fn new(
        u: &mut Universe,
        any: SymbolId,
        object: SymbolId,
        runtime: SymbolId,
    ) -> Result<Equality, String> {
        let java_lang = package(u, "java.lang")?;
        let java_io = package(u, "java.io")?;
        let scala = package(u, "scala")?;
        let object = type_ref(u, object);
        let boolean = member(u, scala, "Boolean", Kind::Class)?;
        let boolean = type_ref(u, boolean);
        let int = member(u, scala, "Int", Kind::Class)?;
        let int = type_ref(u, int);
        let boxes_runtime = java_class(u, runtime, "BoxesRunTime");
        let info = method(u, &[object, object], boolean);
        let equals = java_static(u, boxes_runtime, "equals", Kind::Def, info);
        let statics = java_class(u, runtime, "Statics");
        let info = method(u, &[object], int);
        let any_hash = java_static(u, statics, "anyHash", Kind::Def, info);
        Ok(Equality {
            any_equals: member(u, any, "equals", Kind::Def)?,
            boxes_runtime,
            equals,
            statics,
            any_hash,
            number: member(u, java_lang, "Number", Kind::Class)?,
            character: member(u, java_lang, "Character", Kind::Class)?,
            boolean: member(u, java_lang, "Boolean", Kind::Class)?,
            serializable: member(u, java_io, "Serializable", Kind::Class)?,
            comparable: member(u, java_lang, "Comparable", Kind::Class)?,
        })
    }
}

fn method(u: &mut Universe, params: &[TypeId], result: TypeId) -> TypeId {
    let binder = u.fresh_binder();
    let params = params.iter().enumerate().map(|(i, &t)| (format!("x${i}"), t)).collect();
    u.intern_type(Type::Method { binder, params, result, implicit: false })
}
