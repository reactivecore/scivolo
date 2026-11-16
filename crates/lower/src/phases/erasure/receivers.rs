// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/Erasure.scala

use crate::definitions::type_ref;
use crate::phases::select_static::is_scala_static;
use crate::{Lower, Result, unsupported};
use tasty::tags::PROTECTED;
use tasty::{Erased, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// Determines the erased type the receiver `qualifier` of selection `sel` is adapted to: the class
/// owning the selected member, or the receiver's own type if the owner is a Java class the code
/// being erased cannot name (dotty's `isJvmAccessible` in `typedSelect`); `None` for a `super`
/// receiver, an `@static` member, a member of `Array` or a member outside classes. Refuses a
/// member of an inaccessible Java class selected from a receiver of that class, as dotty does.
pub fn receiver_type(l: &mut Lower, sel: TNodeId, qualifier: TNodeId) -> Result<Option<Erased>> {
    if matches!(l.tpd.tree(qualifier), Tree::Super { .. }) {
        return Ok(None);
    }
    let Some(member) = l.tpd.sym(sel).filter(|&m| !is_scala_static(l, m)) else { return Ok(None) };
    let Some(owner) = l.u.symbol(member).owner.filter(|&o| l.u.is_class(o)) else { return Ok(None) };
    if l.u.full_name(owner) == "scala.Array" {
        let array = l.u.widen(l.tpd.tpe(qualifier));
        let e = l.u.erase(array);
        return Ok(matches!(e, Erased::Array(_)).then_some(e));
    }
    let owner_type = type_ref(l.u, owner);
    let owner_erased = l.u.erase_boxed(owner_type);
    if is_jvm_accessible(l, owner) {
        return Ok(Some(owner_erased));
    }
    let widened = l.u.widen(l.tpd.tpe(qualifier));
    let erased = l.u.erase_boxed(widened);
    if erased == owner_erased {
        let name = l.u.full_name(owner);
        return unsupported(l.tpd.span(sel), format!("a member of Java class {name}, not accessible here"));
    }
    Ok(Some(erased))
}

/// Whether the code being erased may name class `c` in a cast: a Scala class, a Java-defined
/// `protected` class, or a Java class it may access (dotty's `isJvmAccessible`, through the
/// prefix `c.owner.this`); a Scala-defined `java.*` class (Scala Native's javalib) counts as Java.
fn is_jvm_accessible(l: &mut Lower, c: SymbolId) -> bool {
    let name = l.u.full_name(c);
    let jdk = l.u.may_be_java(c) && (name.starts_with("java.") || name.starts_with("javax."));
    let java = l.u.is_java_defined(c);
    if !jdk && !java || java && l.u.symbol(c).flags.has(PROTECTED) {
        return true;
    }
    let site = l.erasure_owner.unwrap_or(l.u.root());
    l.u.is_class_accessible_from(c, site)
}
