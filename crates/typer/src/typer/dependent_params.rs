//! Parameter and result types an application leaves to dotty: by-name, dependent.

use super::Typer;
use super::stability::is_stable_path;
use tasty::{Binder, Kind, SymbolId, Type, TypeId, Universe};

impl Typer<'_> {
    /// Whether parameter type `p` is by-name or refers to a parameter.
    pub fn is_special_param(&mut self, p: TypeId) -> bool {
        let mut special = false;
        self.u.map_type(p, &mut |u, part| {
            special |= matches!(u.types().get(part), Type::ByName(_) | Type::Param(..));
            None
        });
        special
    }

    pub fn applied_result(
        &mut self,
        t: TypeId,
        params: (Binder, &[String]),
        method: Option<SymbolId>,
        args: &[TypeId],
    ) -> Result<TypeId, &'static str> {
        applied_result(self.u, t, params, method, args)
    }

    pub fn mentions_params(&mut self, t: TypeId, binder: Binder, method: Option<SymbolId>) -> bool {
        mentions_params(self.u, t, binder, method)
    }
}

/// Result `t` of the method type of binder `binder` and parameters `names`, of method `method`,
/// applied to arguments of types `args`: its references to the parameters replaced by the
/// arguments' types; `Err` if one of those is not stable, which dotty approximates by a skolem
/// (`safeSubstParams`).
pub fn applied_result(
    u: &mut Universe,
    t: TypeId,
    (binder, names): (Binder, &[String]),
    method: Option<SymbolId>,
    args: &[TypeId],
) -> Result<TypeId, &'static str> {
    if !mentions_params(u, t, binder, method) {
        return Ok(t);
    }
    let mut used = Vec::new();
    let out = u.map_type(t, &mut |u, part| {
        let i = match *u.types().get(part) {
            Type::Param(b, i) if b == binder => i as usize,
            Type::Sym(None, p) if u.symbol(p).kind == Kind::Param && u.symbol(p).owner == method => {
                names.iter().position(|n| *n == u.symbol(p).name)?
            }
            _ => return None,
        };
        used.push(*args.get(i)?);
        args.get(i).copied()
    });
    match used.into_iter().all(|a| is_stable_path(u, a)) {
        true => Ok(out),
        false => Err("dependent result of an unstable argument"),
    }
}

/// Whether type `t` refers to a value parameter of the method type of binder `binder`, or of
/// method `method`.
pub fn mentions_params(u: &mut Universe, t: TypeId, binder: Binder, method: Option<SymbolId>) -> bool {
    let mut found = false;
    u.map_type(t, &mut |u, part| {
        match *u.types().get(part) {
            Type::Param(b, _) if b == binder => found = true,
            Type::Sym(_, s) if u.symbol(s).kind == Kind::Param && method.is_some() => {
                found |= u.symbol(s).owner == method;
            }
            _ => {}
        }
        None
    });
    found
}
