// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/ProtoTypes.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The result of an implicit method with its implicit parameters approximated by wildcards
//! (dotty's `resultTypeApprox` with `wildcardOnly`).

use super::Typer;
use super::dependent_params::clause_param;
use tasty::{Kind, NameId, SymbolId, Type, TypeId, Universe};

impl Typer<'_> {
    /// Gives the result of method type `t` of `meth` after its leading implicit clauses, each of
    /// their parameters replaced by a wildcard of its type, a selection on such a wildcard by a
    /// wildcard, as are its applications (`fn.R[Y]` for `using fn: Fn` becomes `?`); `None` where
    /// a higher-kinded type member is selected on such a parameter and not applied (`Bar[fn.F]`
    /// with `F[_]` kept unapplied).
    pub fn wildcard_result(&mut self, mut t: TypeId, meth: Option<SymbolId>) -> Option<TypeId> {
        while let Type::Method { binder, params, result, implicit: true, .. } = *self.u.ty(t) {
            let params = self.u.params_of(params).to_vec();
            let names: Vec<NameId> = params.iter().map(|&(n, _)| n).collect();
            let mut wilds = Vec::with_capacity(params.len());
            for &(_, p) in &params {
                let p = self.u.by_name(p).unwrap_or(p);
                let bounds = self.u.intern_type(Type::Bounds(p, p));
                wilds.push(self.u.intern_type(Type::Wildcard(Some(bounds))));
            }
            let param = |u: &Universe, part: TypeId| clause_param(u, part, (binder, &names), meth);
            t = wildcarded(self.u, result, &param, &wilds)?;
        }
        Some(t)
    }
}

/// `t` with each parameter `param` numbers replaced by its wildcard, selections and their
/// applications on one by an unbounded wildcard; `None` for an unapplied higher-kinded selection.
fn wildcarded(
    u: &mut Universe,
    t: TypeId,
    param: &dyn Fn(&Universe, TypeId) -> Option<usize>,
    wilds: &[TypeId],
) -> Option<TypeId> {
    let mut hk = false;
    let mapped = u.map_type(t, &mut |u, part| {
        if let Some(i) = param(u, part) {
            return wilds.get(i).copied();
        }
        let selection = match u.applied(part) {
            Some((tycon, _)) if on_param(u, tycon, param) => true,
            _ => {
                let on = on_param(u, part, param);
                let s = u.sym_of(part).filter(|_| on);
                hk |= s.is_some_and(|s| u.symbol(s).kind == Kind::Type && !u.type_params(s).is_empty());
                on
            }
        };
        selection.then(|| u.intern_type(Type::Wildcard(None)))
    });
    (!hk).then_some(mapped)
}

/// Whether `t` selects a member on a parameter `param` numbers, directly or through a path.
fn on_param(u: &Universe, t: TypeId, param: &dyn Fn(&Universe, TypeId) -> Option<usize>) -> bool {
    match *u.ty(t) {
        Type::Sym(Some(pre), _) | Type::TypeRef(pre, _) | Type::TermRef(pre, _) => {
            param(u, pre).is_some() || on_param(u, pre, param)
        }
        _ => false,
    }
}
