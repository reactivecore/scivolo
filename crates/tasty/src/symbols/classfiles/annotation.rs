// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/classfile/ClassfileParser.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! An annotation class of a class file: its constructor and its retention.

use super::super::builtin::synthetic;
use super::super::{Kind, SymbolId, Universe};
use crate::tags::SYNTHETIC;
use crate::types::Type;
use classfile::{ACC_ANNOTATION, ClassInfo};
use names::nme;

/// The descriptor of `java.lang.annotation.Retention`.
const RETENTION: &str = "Ljava/lang/annotation/Retention;";

/// Where the annotations of an annotation class are kept (`java.lang.annotation.RetentionPolicy`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retention {
    Source,
    Class,
    Runtime,
}

/// Determines the retention of annotation class `info` from its `@Retention`, `CLASS` without one
/// (dotty's `retentionPolicyOf`).
fn retention(info: &ClassInfo) -> Retention {
    let retention = info.annotations.iter().find(|a| a.descriptor == RETENTION);
    let policy = retention.and_then(|a| a.enum_elements.iter().find(|(n, _)| n == "value"));
    match policy.map(|(_, c)| c.as_str()) {
        Some("SOURCE") => Retention::Source,
        Some("RUNTIME") => Retention::Runtime,
        _ => Retention::Class,
    }
}

/// Records the retention of `class` entered from `info` and enters its constructor if `info` is
/// an annotation class.
pub fn enter(u: &mut Universe, class: SymbolId, info: &ClassInfo, members: &[SymbolId]) {
    if info.access & ACC_ANNOTATION != 0 {
        enter_constructor(u, class, members);
        u.classfile_annotations.insert(class, retention(info));
    }
}

/// Enters the constructor of annotation class `class`: a parameter per element, an instance
/// method of `members`, of its result type, returning the class (dotty's
/// `addAnnotationConstructor`).
fn enter_constructor(u: &mut Universe, class: SymbolId, members: &[SymbolId]) {
    let mut params = Vec::new();
    for &m in members {
        if u.symbol(m).owner == Some(class) && u.symbol(m).kind == Kind::Def {
            let info = u.info(m);
            let result = u.result_type(info);
            params.push((u.types.intern_name(&u.symbol(m).name.clone()), result));
        }
    }
    let result = u.types.intern(Type::Sym(None, class));
    let binder = u.fresh_binder();
    let info = u.types.intern_method(binder, &params, result, false, false);
    let constructor = synthetic(u, class, nme::CONSTRUCTOR, Kind::Def, &[SYNTHETIC]);
    u.infos.insert(constructor, info);
}
