//! Parameter and result types an application leaves to dotty: by-name, dependent.

use super::Typer;
use tasty::{Binder, Kind, SymbolId, Type, TypeId};

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

    /// Result `t` of the method type of binder `binder` and parameters `names`, of method
    /// `method`, applied to arguments of types `args`: its references to the parameters replaced
    /// by the arguments' types, which must be stable (dotty's `instantiate`).
    pub fn applied_result(
        &mut self,
        t: TypeId,
        (binder, names): (Binder, &[String]),
        method: Option<SymbolId>,
        args: &[TypeId],
    ) -> Result<TypeId, &'static str> {
        if !self.mentions_params(t, binder, method) {
            return Ok(t);
        }
        let mut unstable = false;
        let out = self.u.map_type(t, &mut |u, part| {
            let i = match *u.types().get(part) {
                Type::Param(b, i) if b == binder => i as usize,
                Type::Sym(None, p) if u.symbol(p).kind == Kind::Param && u.symbol(p).owner == method => {
                    names.iter().position(|n| *n == u.symbol(p).name)?
                }
                _ => return None,
            };
            let arg = *args.get(i)?;
            unstable |= !matches!(u.types().get(arg), Type::Sym(_, s) if u.symbol(*s).kind.is_term())
                && !matches!(u.types().get(arg), Type::This(_) | Type::Constant(..) | Type::Str(_));
            Some(arg)
        });
        match unstable {
            true => Err("dependent result of an unstable argument"),
            false => Ok(out),
        }
    }

    /// Whether type `t` refers to a value parameter of the method type of binder `binder`, or of
    /// method `method`.
    pub fn mentions_params(&mut self, t: TypeId, binder: Binder, method: Option<SymbolId>) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
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
}
