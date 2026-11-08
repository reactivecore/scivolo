// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Overloads among methods and a value with `apply` methods (dotty's `resolveOverloaded` with
//! `applyMembers` and `retract`).

use super::Typer;
use syntax::ast::Tree;
use tasty::{Kind, SymbolId, TypeId};

const VALUE_APPLY: &str = "overloaded value with an apply method";

impl Typer<'_> {
    /// The alternative of `methods` and value `value`, whose `apply` methods stand for it, that
    /// the arguments of `proto` select; `None` if no `apply` method takes them, leaving the
    /// choice to the methods.
    pub fn applied_value(
        &mut self,
        value: SymbolId,
        methods: &[SymbolId],
        prefix: Option<TypeId>,
        proto: usize,
    ) -> Result<Option<SymbolId>, &'static str> {
        let (args, ctx) = (self.protos[proto].args.clone(), self.protos[proto].ctx);
        let simple = args.iter().all(|&a| {
            self.unknown_param_function(a).is_none() && !matches!(self.ast.tree(a), Tree::NamedArg { .. })
        });
        if !simple || self.protos[proto].using {
            return Err(VALUE_APPLY);
        }
        let vt = self.ref_type(prefix, value);
        let applies: Vec<SymbolId> = self
            .type_members(vt, "apply")
            .into_iter()
            .filter(|&s| self.u.symbol(s).kind == Kind::Def)
            .collect();
        let arg_types = self.proto_arg_types(proto, methods, prefix)?;
        let alts = methods.iter().map(|&m| (m, prefix)).chain(applies.iter().map(|&a| (a, Some(vt))));
        let mut fitting = Vec::new();
        for (alt, pre) in alts.collect::<Vec<_>>() {
            if self.applicable_to_proto(alt, pre, (proto, &arg_types), false, ctx).ok_or(VALUE_APPLY)? {
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
                beats_all &= self.as_good(a, b, ctx)? && !self.as_good(b, a, ctx)?;
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

    /// Whether method `a` is as good as `b` (dotty's `isAsGood` of methods): `b` takes the
    /// parameter types of `a`, both of the same owner.
    fn as_good(
        &mut self,
        (a, pa): (SymbolId, Option<TypeId>),
        (b, pb): (SymbolId, Option<TypeId>),
        ctx: crate::ctx::Ctx,
    ) -> Result<bool, &'static str> {
        let (oa, ob) = (self.u.symbol(a).owner, self.u.symbol(b).owner);
        let related = |t: &mut Self| match (oa, ob) {
            (Some(x), Some(y)) => x == y || t.u.derives_from(x, y) || t.u.derives_from(y, x),
            _ => false,
        };
        if related(self) && oa != ob {
            return Err(VALUE_APPLY);
        }
        let Some((formals, false)) = self.param_types(a, pa, false) else { return Err(VALUE_APPLY) };
        self.applicable(b, pb, &formals, false, ctx).ok_or(VALUE_APPLY)
    }
}
