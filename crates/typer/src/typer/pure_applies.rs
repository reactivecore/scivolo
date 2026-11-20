// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala

use super::Typer;
use super::java_classes::is_java_member;
use super::purity::has_constant_type;
use crate::tpd::TNodeId;
use names::nme;
use tasty::{Kind, SymbolId};

/// Determines whether application `tree` of `callee` has no effect (dotty's `isPureApply`): a
/// constant operation of a primitive class, `String` or a pure member of `Any`, a stable
/// constructor of a class outside the JDK, or the synthetic `apply` of a case class that
/// initializes nothing.
pub fn is_pure_apply(t: &mut Typer, tree: TNodeId, callee: Option<SymbolId>) -> bool {
    let Some(s) = callee else { return false };
    if t.u.symbol(s).name == nme::CONSTRUCTOR {
        return t.is_stable_member(s) && !is_java_member(t.u, s);
    }
    if is_pure_case_apply(t, s) {
        return true;
    }
    has_constant_type(&t.tpd, t.u, tree) && t.u.is_known_pure_op(s)
}

/// dotty's `isPureSyntheticCaseApply`.
fn is_pure_case_apply(t: &mut Typer, s: SymbolId) -> bool {
    let sym = t.u.symbol(s);
    if sym.name != nme::APPLY || !sym.flags.has(tasty::tags::SYNTHETIC) {
        return false;
    }
    let Some(module) = sym.owner else { return false };
    let Some(name) = tasty::module_name(&t.u.symbol(module).name).map(str::to_owned) else {
        return false;
    };
    let Some(owner) = t.u.symbol(module).owner else { return false };
    let classes = t.u.decls_named(owner, &name);
    classes.into_iter().any(|c| {
        let flags = t.u.symbol(c).flags;
        t.u.symbol(c).kind == Kind::Class
            && flags.has(tasty::tags::CASE)
            && !flags.has(tasty::tags::TRAIT)
            && (t.is_no_inits(c) || t.u.is_assured_no_inits(c))
    })
}
