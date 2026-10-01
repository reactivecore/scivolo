//! The result types of default getters, `m$default$N`: the type of `m`'s parameter `N` (dotty's
//! `defaultParamType` in `Namer.inferredResultType`).

use crate::typer::Typer;
use syntax::ast::flags as f;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The type of the parameter default getter `getter` supplies, if it is one; references to the
    /// method's type parameters become references to the getter's same-named ones.
    pub fn default_param_type(&mut self, getter: SymbolId) -> Option<TypeId> {
        let name = self.u.symbol(getter).name.clone();
        let (method, index) = name.rsplit_once("$default$")?;
        let index: usize = index.parse().ok()?;
        let owner = self.u.symbol(getter).owner?;
        let mut candidates: Vec<SymbolId> = self.u.symbol(owner).decls.clone();
        candidates.retain(|&d| self.u.symbol(d).kind == Kind::Def && self.u.symbol(d).name == method);
        candidates.extend(self.constructor_of_companion(owner, method));
        for m in candidates {
            self.complete(m);
            let params: Vec<SymbolId> = self
                .u
                .symbol(m)
                .decls
                .iter()
                .copied()
                .filter(|&p| self.u.symbol(p).kind == Kind::Param)
                .collect();
            let Some(&param) = params.get(index.checked_sub(1)?) else { continue };
            if !self.syms.flags.get(&param).is_some_and(|fl| fl.is(f::HAS_DEFAULT)) {
                continue;
            }
            let info = self.u.info(param);
            return Some(self.with_getter_type_params(info, m, getter));
        }
        None
    }

    /// The primary constructor of the class `owner`, an object, accompanies, for `<init>`'s
    /// default getters, which live in the companion.
    fn constructor_of_companion(&mut self, owner: SymbolId, method: &str) -> Option<SymbolId> {
        if method != "<init>" {
            return None;
        }
        self.companion_constructor(owner)
    }

    /// `t` with references to the type parameters of `method` made references to the same-named
    /// type parameters of `getter`.
    fn with_getter_type_params(&mut self, t: TypeId, method: SymbolId, getter: SymbolId) -> TypeId {
        let getter_tparams: Vec<SymbolId> = self.u.symbol(getter).decls.clone();
        self.u.map_type(t, &mut |u, x| {
            let Type::Sym(None, s) = *u.types().get(x) else { return None };
            if u.symbol(s).owner != Some(method) || u.symbol(s).kind != Kind::TypeParam {
                return None;
            }
            let own = getter_tparams.iter().find(|&&g| u.symbol(g).name == u.symbol(s).name)?;
            Some(u.intern_type(Type::Sym(None, *own)))
        })
    }
}
