// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Overloads among methods and a value with `apply` methods (dotty's `resolveOverloaded` with
//! `applyMembers` and `retract`).

use super::applicability::{applicable, param_types};
use super::by_result::ParamRefs;
use super::mapped::leading_implicit;
use super::named::applicable_to_proto;
use super::resolve::result_is_method;
use super::shapes::unknown_param_function;
use super::{Resolution, VALUE_APPLY};
use crate::ctx::Ctx;
use crate::typer::{Pt, Typer};
use names::nme;
use syntax::ast::Tree;
use tasty::{Kind, SymbolId, TypeId};

/// An alternative and the type it is a member of.
type Member = (SymbolId, Option<TypeId>);

impl Resolution {
    /// Chooses among `methods` and value `value`, whose `apply` methods stand for it, the
    /// alternative the arguments of `proto` select, as `f(1)` among `def f(s: String)` and
    /// `val f: Int => Int`; `None` if no `apply` method takes them, leaving the choice to the
    /// methods.
    pub fn applied_value(
        self,
        t: &mut Typer,
        value: SymbolId,
        methods: &[SymbolId],
        proto: usize,
    ) -> Result<Option<SymbolId>, &'static str> {
        let (args, ctx) = (t.protos[proto].args.clone(), t.protos[proto].ctx);
        let simple = args.iter().all(|&a| {
            unknown_param_function(t, a).is_none() && !matches!(t.ast.tree(a), Tree::NamedArg { .. })
        });
        if !simple || t.protos[proto].using || methods.iter().any(|&m| leading_implicit(t.u, m).is_some()) {
            return Err(VALUE_APPLY);
        }

        let vt = t.ref_type(self.prefix, value);
        let applies: Vec<SymbolId> =
            t.type_members(vt, nme::APPLY).into_iter().filter(|&s| t.u.symbol(s).kind == Kind::Def).collect();
        let arg_types = self.proto_arg_types(t, proto, methods)?;
        let alts: Vec<Member> =
            methods.iter().map(|&m| (m, self.prefix)).chain(applies.iter().map(|&a| (a, Some(vt)))).collect();
        let mut fitting = Vec::new();
        for &(alt, pre) in &alts {
            if applicable_to_proto(t, alt, pre, (proto, &arg_types), false, ctx).ok_or(VALUE_APPLY)? {
                fitting.push((alt, pre));
            }
        }
        if !fitting.iter().any(|&(_, pre)| pre == Some(vt)) {
            return if fitting.is_empty() { Err(VALUE_APPLY) } else { Ok(None) };
        }

        let best = match most_specific(t, &fitting, ctx)? {
            Some(best) => best,
            None => {
                let deeper = self.fitting_result(t, (&alts, &fitting), proto)?;
                match deeper[..] {
                    [only] => only,
                    _ => most_specific(t, &deeper, ctx)?.ok_or(VALUE_APPLY)?,
                }
            }
        };
        match self.adapted_member(t, best, &alts, proto)? {
            (_, pre) if pre == Some(vt) => Ok(Some(value)),
            (m, _) => Ok(Some(m)),
        }
    }

    /// Gives `chosen`, or if its result does not conform to the expected result type of `proto`,
    /// the only other alternative of `alts` whose result does (dotty's `adaptByResult`); refuses
    /// several.
    fn adapted_member(
        self,
        t: &mut Typer,
        chosen: Member,
        alts: &[Member],
        proto: usize,
    ) -> Result<Member, &'static str> {
        let (Pt::Type(pt), ctx) = (t.protos[proto].pt, t.protos[proto].ctx) else { return Ok(chosen) };
        let fits = |t: &mut Typer, (alt, prefix): Member| {
            let r =
                Resolution { prefix, ..self }.result_conforms(t, alt, pt, (true, ctx), ParamRefs::Skolems);
            r.ok_or(VALUE_APPLY)
        };
        if fits(t, chosen)? {
            return Ok(chosen);
        }
        let mut conforming = Vec::new();
        for &alt in alts.iter().filter(|&&a| a != chosen) {
            if fits(t, alt)? {
                conforming.push(alt);
            }
        }
        match conforming[..] {
            [] => Ok(chosen),
            [alt] => Ok(alt),
            _ => Err(VALUE_APPLY),
        }
    }

    /// Keeps the alternatives of `fitting`, of `alts`, whose results conform to the expected
    /// result type of `proto` (dotty's `deepenProto`); refuses if some of `alts` have default
    /// arguments or curried results, which dotty prefers the others to first.
    fn fitting_result(
        self,
        t: &mut Typer,
        (alts, fitting): (&[Member], &[Member]),
        proto: usize,
    ) -> Result<Vec<Member>, &'static str> {
        let (Pt::Type(pt), ctx) = (t.protos[proto].pt, t.protos[proto].ctx) else { return Err(VALUE_APPLY) };
        if alts.iter().any(|&(alt, _)| t.has_defaults(alt) || result_is_method(&mut t.types(), alt)) {
            return Err(VALUE_APPLY);
        }
        let mut kept = Vec::new();
        for &(alt, prefix) in fitting {
            let fits =
                Resolution { prefix, ..self }.result_conforms(t, alt, pt, (false, ctx), ParamRefs::TypeVars);
            if fits.ok_or(VALUE_APPLY)? {
                kept.push((alt, prefix));
            }
        }
        Ok(kept)
    }
}

/// Finds the alternative of `fitting` as good as each other one and not they as good as it.
fn most_specific(t: &mut Typer, fitting: &[Member], ctx: Ctx) -> Result<Option<Member>, &'static str> {
    let mut best = None;
    for &a in fitting {
        let mut beats_all = true;
        for &b in fitting.iter().filter(|&&b| b != a) {
            beats_all &= as_good(t, a, b, ctx)? && !as_good(t, b, a, ctx)?;
        }
        if beats_all {
            best = Some(a);
        }
    }
    Ok(best)
}

/// Checks whether method `a` is as good as `b` (dotty's `isAsGood` of methods): `b` takes the
/// parameter types of `a`, both of owners neither of which, nor of their companion classes if
/// modules, derives from the other (dotty's `compareOwner` 0).
fn as_good(t: &mut Typer, (a, pa): Member, (b, pb): Member, ctx: Ctx) -> Result<bool, &'static str> {
    let (Some(oa), Some(ob)) = (t.u.symbol(a).owner, t.u.symbol(b).owner) else {
        return Err(VALUE_APPLY);
    };
    if oa != ob {
        let classes = |t: &mut Typer, o: SymbolId| {
            let companion = t.u.is_module_class(o).then(|| t.u.companion_class(o)).flatten();
            [Some(o), companion].into_iter().flatten().collect::<Vec<_>>()
        };
        let (ca, cb) = (classes(t, oa), classes(t, ob));
        if ca.iter().any(|&x| cb.iter().any(|&y| t.u.derives_from(x, y) || t.u.derives_from(y, x))) {
            return Err(VALUE_APPLY);
        }
    }
    let Some((formals, false)) = param_types(t, a, pa, false) else { return Err(VALUE_APPLY) };
    applicable(t, b, pb, &formals, false, ctx).ok_or(VALUE_APPLY)
}
