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

/// The descriptor of `java.lang.annotation.Repeatable`.
const REPEATABLE: &str = "Ljava/lang/annotation/Repeatable;";

/// Where the annotations of an annotation class are kept (`java.lang.annotation.RetentionPolicy`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retention {
    Source,
    Class,
    Runtime,
}

/// Determines the retention of annotation class `info` from its `@Retention`, `RUNTIME` without
/// one (dotty's `retentionPolicyOf` and `isRuntimeVisible`).
fn retention(info: &ClassInfo) -> Retention {
    let Some(retention) = info.annotations.iter().find(|a| a.descriptor == RETENTION) else {
        return Retention::Runtime;
    };
    let policy = retention.enum_elements.iter().find(|(n, _)| n == "value");
    match policy.map(|(_, c)| c.as_str()) {
        Some("SOURCE") => Retention::Source,
        Some("RUNTIME") => Retention::Runtime,
        _ => Retention::Class,
    }
}

/// Records the retention of `class` entered from `info` and the elements of `members` with a
/// default value, and enters its constructor, if `info` is an annotation class.
pub fn enter(u: &mut Universe, class: SymbolId, info: &ClassInfo, members: &[SymbolId]) {
    if info.access & ACC_ANNOTATION != 0 {
        enter_constructor(u, class, members);
        u.classfile_annotations.insert(class, retention(info));
        let repeatable = info.annotations.iter().find(|a| a.descriptor == REPEATABLE);
        let container = repeatable.and_then(|a| a.class_elements.iter().find(|(n, _)| n == "value"));
        if let Some(internal) = container.and_then(|(_, d)| d.strip_prefix('L')?.strip_suffix(';')) {
            u.repeatable_containers.insert(class, internal.to_owned());
        }
        let defaults = info.methods.iter().filter(|m| m.annotation_default).map(|m| m.name.as_str());
        let defaults: Vec<&str> = defaults.collect();
        for &m in members {
            if u.symbol(m).owner == Some(class) && defaults.contains(&u.symbol(m).name.as_str()) {
                u.annotation_defaults.insert(m);
            }
        }
    }
}

impl Universe {
    /// The internal name of the container class of annotation class `class` of a class file if
    /// it is `@Repeatable`.
    pub fn repeatable_container(&self, class: SymbolId) -> Option<&str> {
        self.repeatable_containers.get(&class).map(String::as_str)
    }

    /// Determines the retention of the annotation class of internal name `internal` read from
    /// its class file.
    pub fn annotation_retention(&mut self, internal: &str) -> Option<Retention> {
        self.java_class_info(internal).map(|info| retention(&info))
    }

    /// Determines whether element `name` of annotation class `class` of a class file has a
    /// default value (dotty's `AnnotationDefaultAnnot`).
    pub fn has_annotation_default(&self, class: SymbolId, name: &str) -> bool {
        self.symbol(class)
            .decls
            .iter()
            .any(|&m| self.annotation_defaults.contains(&m) && self.symbol(m).name == name)
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
