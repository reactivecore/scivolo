//! Java classes of the Scala library without TASTy, entered with their parents and members:
//! `scala.math.ScalaNumber`.

use super::any_members::{class_type, method};
use super::builtin::synthetic;
use super::{Kind, SymbolId, Universe};
use crate::tags::{ABSTRACT, PROTECTED};
use crate::types::Type;

pub fn enter(u: &mut Universe, object: SymbolId) {
    let Some(math) = u.package("scala.math") else { return };
    if u.pending.get(&math).is_some_and(|f| f.iter().any(|(stem, _)| stem == "ScalaNumber")) {
        return;
    }
    let number = synthetic(u, math, "ScalaNumber", Kind::Class, &[ABSTRACT]);
    let parent = class_type(u, "java.lang", "Number");
    u.parents.insert(number, vec![parent]);
    let boolean = class_type(u, "scala", "Boolean");
    let object = u.types.intern(Type::Sym(None, object));
    let this = u.types.intern(Type::Sym(None, number));
    method(u, number, "<init>", &[], this, &[]);
    method(u, number, "isWhole", &[], boolean, &[PROTECTED]);
    method(u, number, "underlying", &[], object, &[]);
}
