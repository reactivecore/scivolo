//! Choosing among the alternatives a name denotes by the shape of an application: values
//! without `apply` dropped, methods by the number of arguments they take (dotty's
//! `narrowBySize` with `sizeFits`), before overload resolution.

use super::{Pt, Typer};
use tasty::{Kind, SymbolId, TypeId};

impl Typer<'_> {
    /// The alternative of `syms`, members of `prefix`, that `pt` selects: the only one, the only
    /// method taking as many arguments as `pt` supplies, or the one overload resolution picks.
    pub fn choose(
        &mut self,
        syms: &[SymbolId],
        prefix: Option<TypeId>,
        pt: Pt,
    ) -> Result<SymbolId, &'static str> {
        if let [s] = syms {
            return Ok(*s);
        }
        let Pt::Fun(proto) = pt else { return Err("overloaded reference") };
        let mut methods = Vec::new();
        for &s in syms {
            self.complete(s);
            if self.u.symbol(s).kind == Kind::Def {
                methods.push(s);
                continue;
            }
            let value = self.ref_type(prefix, s);
            if !self.type_members(value, "apply").is_empty() {
                return Err("overloaded value with an apply method");
            }
        }
        if let [s] = methods[..] {
            return Ok(s);
        }
        let n = self.protos[proto].args.len();
        let mut fitting = Vec::new();
        for s in methods {
            let info = self.u.info(s);
            let Some((_, ps)) = self.u.param_clauses(info).into_iter().next() else { continue };
            if ps.iter().any(|&p| self.is_special_param(p)) || self.has_defaults(s) {
                return Err("overloads with default or by-name parameters");
            }
            let repeated = ps.last().is_some_and(|&p| self.repeated_elem(p).is_some());
            let fits = if repeated { n + 1 >= ps.len() } else { n == ps.len() };
            if fits {
                fitting.push(s);
            }
        }
        match fitting.as_slice() {
            [s] => Ok(*s),
            [] => Err("no alternative with that many arguments"),
            _ => self.resolve_overloaded(&fitting, prefix, proto),
        }
    }

    /// Whether a method named as `s` in its owner has default arguments.
    fn has_defaults(&mut self, s: SymbolId) -> bool {
        let Some(owner) = self.u.symbol(s).owner else { return false };
        let prefix = format!("{}$default$", self.u.symbol(s).name);
        let decls = self.u.decls(owner).unwrap_or_default();
        decls.iter().any(|&d| self.u.symbol(d).name.starts_with(&prefix))
    }
}
