// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/VarianceChecker.scala
//! The messages of variance errors (dotty's `Traverser.checkVariance`).

use super::super::variance_ops::{VarianceOps, declared, label};
use syntax::ast::flags as f;
use tasty::{Kind, SymbolId, Type, TypeId};

const ENUM_EXAMPLE: &str = "See an example at https://docs.scala-lang.org/scala3/reference/enums/adts.html#parameter-variance-of-enums";

/// Writes dotty's error for type parameter `tvar` occurring in the type of `sym` where variance
/// `required` is, as "covariant type A occurs in contravariant position in type A of parameter
/// a"; of a class, `unchecked` are its parents annotated `@uncheckedVariance`. It adds dotty's
/// hints for the synthetic type parameters of enum cases and for private parameter accessors.
pub fn variance_message(
    ops: &mut VarianceOps,
    sym: SymbolId,
    tvar: SymbolId,
    required: u8,
    unchecked: &[TypeId],
) -> String {
    let info = ops.u.info(sym);
    let shown = match ops.u.symbol(sym).kind {
        Kind::Class => {
            let parents: Vec<String> = (ops.u.parents(sym).into_iter())
                .map(|p| match unchecked.contains(&p) {
                    true => format!("{} @uncheckedVariance", ops.u.show_user(p)),
                    false => ops.u.show_user(p),
                })
                .collect();
            format!("{} {{...}}", parents.join(" with "))
        }
        Kind::Def if !matches!(ops.u.types().get(info), Type::Method { .. } | Type::Poly { .. }) => {
            format!("=> {}", ops.u.show_user(info))
        }
        _ => ops.u.show_user(info),
    };
    let (own, at) = (label(declared(ops.u, tvar)), label(required));
    let (tname, described) = (ops.u.symbol(tvar).name.clone(), ops.syms.described(ops.u, sym));
    let mut msg = format!("{own} type {tname} occurs in {at} position in type {shown} of {described}");
    let towner = ops.u.symbol(tvar).owner;
    let enum_case = towner.filter(|&o| ops.syms.flags_of(o).is_all_of(f::ENUM_CASE) && ops.u.is_class(o));
    if let Some(case) = enum_case
        && ops.syms.flags_of(tvar).is(f::SYNTHETIC)
    {
        let case = &ops.u.symbol(case).name;
        msg.push_str(&format!(
            "\nenum case {case} requires explicit declaration of type {tname} to resolve this issue.\n{ENUM_EXAMPLE}"
        ));
    }
    let flags = ops.syms.flags_of(sym);
    if flags.is(f::PARAM_ACCESSOR) && flags.is(f::PRIVATE) {
        let (kw, fix) = match flags.is(f::MUTABLE) {
            true => ("var", " and add\na field inside the class instead"),
            false => ("val", ""),
        };
        let name = &ops.u.symbol(sym).name;
        msg.push_str(&format!(
            "\n\nImplementation limitation: private {kw} parameters cannot be inferred to be local\nand therefore are always variance-checked.\n\nPotential fix: remove the private {kw} modifiers on the parameter {name}{fix}."
        ));
    }
    msg
}
