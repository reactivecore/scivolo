//! Java classes of the Scala library without TASTy, entered with their parents and members:
//! `scala.math.ScalaNumber`.

use super::any_members::{class_type, method};
use super::builtin::synthetic;
use super::{Kind, Universe};
use crate::tags::{ABSTRACT, PROTECTED};

pub fn enter(u: &mut Universe) {
    let Some(math) = u.package("scala.math") else { return };
    if u.pending.get(&math).is_some_and(|f| f.iter().any(|(stem, _)| stem == "ScalaNumber")) {
        return;
    }
    let number = synthetic(u, math, "ScalaNumber", Kind::Class, &[ABSTRACT]);
    let parent = class_type(u, "java.lang", "Number");
    u.parents.insert(number, vec![parent]);
    let boolean = class_type(u, "scala", "Boolean");
    let object = class_type(u, "java.lang", "Object");
    method(u, number, "isWhole", &[], boolean, &[PROTECTED]);
    method(u, number, "underlying", &[], object, &[]);
}
