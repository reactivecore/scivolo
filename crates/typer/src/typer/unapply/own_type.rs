// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! The type of an extractor pattern (dotty's `ownType` in `typedUnApply`).

use super::super::Typer;
use super::UnapplyPattern;
use crate::tpd::TNodeId;
use tasty::{Kind, NOTHING, SymbolId, TypeId, TypeVars, Universe};

impl UnapplyPattern {
    /// Infers the type of the pattern whose extractor `unapp` takes `arg_type`: the scrutinee's
    /// type if it conforms, else `arg_type` constrained by it, unless only variant type arguments
    /// of an open class would be, and maximized with the binders added to `unapp`; type variables
    /// from id `vars` on instantiated. Refuses patterns that would constrain invariant type arguments
    /// of an open class or type parameters no GADT bounds narrow.
    pub fn pattern_type(
        &self,
        t: &mut Typer<'_>,
        arg_type: TypeId,
        vars: usize,
        unapp: TNodeId,
    ) -> Result<TypeId, &'static str> {
        let sel = self.sel;
        let mark = t.constraint.mark();
        if t.conforms(sel, arg_type) {
            t.fully_defined_as(arg_type, true)?;
            return Ok(if t.is_bottom_type(sel) { t.zonk(arg_type) } else { t.without_unchecked(sel) });
        }
        t.constraint.rollback(mark);

        let open = t.u.class_symbol(arg_type).is_none_or(|c| {
            let class = t.u.symbol(c);
            !class.is_final() && !class.is_case()
        });
        let variances = t.types().variances(arg_type, None, vars);
        let unconstrained = open && !variances.is_empty();
        if unconstrained && variances.values().any(|&v| v == 0) {
            return Err("invariant type arguments of a pattern of an open class");
        }

        let mut gadt = Vec::new();
        if !unconstrained && !t.constrain_pattern(arg_type, sel)? {
            t.constraint.rollback(mark);
            match t.gadt_vars((arg_type, vars), sel, self.ctx)? {
                Some(narrowed) => gadt = narrowed,
                None => {
                    t.constraint.rollback(mark);
                    if has_abstract_arg(t.u, sel) && !args_unrelated(t.u, arg_type, sel) {
                        return Err("pattern constraining type parameters (GADT)");
                    }
                }
            }
        }

        if !gadt.is_empty() && has_f_bounded_param(t, arg_type) {
            return Err("GADT bounds from a pattern of a class with an F-bounded type parameter");
        }
        let bound = t.maximize_type(arg_type, (vars, &gadt), unapp, self.ctx)?;
        t.narrow_gadt(&gadt)?;
        t.gadt.bind(&bound);
        t.add_binders(unapp, &bound);
        Ok(t.zonk(arg_type))
    }
}

/// Whether each type argument of `pat`'s base type at the class of `sel` applies a type
/// parameter of a class without a lower bound where `sel`'s argument applies a class, so that
/// no comparison of the two constrains anything (`Foo[F[A, B]]` against `Foo[C[X]]` in a class
/// with `F[_, _]`).
fn args_unrelated(u: &mut Universe, pat: TypeId, sel: TypeId) -> bool {
    let sel = u.widen(sel);
    let Some(class) = u.class_symbol(sel) else { return false };
    let Some(sel_args) = u.applied(sel).map(|(_, args)| args.to_vec()) else { return false };
    let Some(base) = u.base_type(pat, class) else { return false };
    let Some(pat_args) = u.applied(base).map(|(_, args)| args.to_vec()) else { return false };
    pat_args.len() == sel_args.len()
        && pat_args.into_iter().zip(sel_args).all(|(p, s)| {
            let (p, s) = (u.dealias(p), u.dealias(s));
            let tycon_sym = |u: &Universe, t| u.applied(t).and_then(|(tycon, _)| u.sym_of(tycon));
            let Some(f) = tycon_sym(u, p) else { return false };
            let class_param = u.symbol(f).kind == Kind::TypeParam
                && u.symbol(f).owner.is_some_and(|o| u.symbol(o).kind == Kind::Class);
            class_param
                && no_lower_bound(u, f)
                && tycon_sym(u, s).is_some_and(|c| u.symbol(c).kind == Kind::Class)
        })
}

/// Whether the lower bound of type parameter `f` is `Nothing`, also as a type lambda's result.
fn no_lower_bound(u: &mut Universe, f: SymbolId) -> bool {
    let info = u.info(f);
    let Some((lo, _)) = u.bounds(info) else { return false };
    let lo = u.lambda(lo).map_or(lo, |l| l.result);
    lo == NOTHING
}

/// Whether the class of `t` has a type parameter whose bounds refer to it (`C[D <: Doc[D]]`).
fn has_f_bounded_param(t: &mut Typer<'_>, tpe: TypeId) -> bool {
    let Some(class) = t.u.class_symbol(tpe) else { return false };
    t.u.type_params(class).into_iter().any(|p| {
        let info = t.u.info(p);
        t.refers_to(info, &[p])
    })
}

/// Whether a type argument of `t`, or a part of one, is a type parameter or an abstract type
/// other than an opaque one, which no GADT bounds narrow.
fn has_abstract_arg(u: &mut Universe, t: TypeId) -> bool {
    let widened = u.widen(t);
    let Some(args) = u.applied(widened).map(|(_, args)| args.to_vec()) else { return false };
    let mut found = false;
    for a in args {
        u.map_type(a, &mut |u, part| {
            if let Some(s) = u.sym_of(part) {
                found |= match u.symbol(s).kind {
                    Kind::TypeParam => true,
                    Kind::Type => {
                        let info = u.info(s);
                        u.type_alias(info).is_none() && !u.symbol(s).is_opaque()
                    }
                    _ => false,
                };
            }
            None
        });
    }
    found
}
