// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Overloads among methods and a value with `apply` methods (dotty's `resolveOverloaded` with
//! `applyMembers` and `retract`).

use super::applicability::{applicable, param_types};
use super::mapped::leading_implicit;
use super::named::applicable_to_proto;
use super::shapes::unknown_param_function;
use super::{Resolution, VALUE_APPLY};
use crate::ctx::Ctx;
use crate::typer::Typer;
use syntax::ast::Tree;
use tasty::{Kind, SymbolId, TypeId, nme};

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
        let alts = methods.iter().map(|&m| (m, self.prefix)).chain(applies.iter().map(|&a| (a, Some(vt))));
        let mut fitting = Vec::new();
        for (alt, pre) in alts.collect::<Vec<_>>() {
            if applicable_to_proto(t, alt, pre, (proto, &arg_types), false, ctx).ok_or(VALUE_APPLY)? {
                fitting.push((alt, pre));
            }
        }
        if !fitting.iter().any(|&(_, pre)| pre == Some(vt)) {
            return if fitting.is_empty() { Err(VALUE_APPLY) } else { Ok(None) };
        }

        let mut best = None;
        for &a in &fitting {
            let mut beats_all = true;
            for &b in fitting.iter().filter(|&&b| b != a) {
                beats_all &= as_good(t, a, b, ctx)? && !as_good(t, b, a, ctx)?;
            }
            if beats_all {
                best = Some(a);
            }
        }
        match best {
            Some((_, pre)) if pre == Some(vt) => Ok(Some(value)),
            Some((m, _)) => Ok(Some(m)),
            None => Err(VALUE_APPLY),
        }
    }
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
