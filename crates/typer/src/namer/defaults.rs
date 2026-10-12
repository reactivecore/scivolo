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

    /// `default_param_type` of `getter` as the expected type of its right-hand side, a by-name
    /// type's result (dotty's `widenExpr`), and whether it was by-name.
    pub fn default_param_proto(&mut self, getter: SymbolId) -> (Option<TypeId>, bool) {
        let Some(d) = self.default_param_type(getter) else { return (None, false) };
        match *self.u.types().get(d) {
            Type::ByName(r) => (Some(r), true),
            _ => (Some(d), false),
        }
    }

    /// The type of a default getter whose right-hand side conforms to its parameter's type `d`:
    /// `d @uncheckedVariance`, `Unit` as itself (dotty's `dealiasIfUnit`).
    pub fn default_getter_type(&mut self, d: TypeId) -> TypeId {
        if self.u.class_symbol(d) == Some(self.defn.unit) {
            return crate::defn::type_ref(self.u, self.defn.unit);
        }
        match self.resolve_full_name("scala.annotation.unchecked.uncheckedVariance", true) {
            Some(c) => {
                let annot = crate::defn::type_ref(self.u, c);
                self.u.intern_type(Type::Annotated(d, annot))
            }
            None => d,
        }
    }

    /// The primary constructor of the class `owner`, an object, accompanies, for `<init>`'s
    /// default getters, which live in the companion.
    fn constructor_of_companion(&mut self, owner: SymbolId, method: &str) -> Option<SymbolId> {
        if method != "<init>" {
            return None;
        }
        self.companion_constructor(owner)
    }

    /// `t` with its references to type parameters of method `sym` made wildcards (dotty's
    /// `wildApprox`); `None` if one of them has bounds other than `Nothing` and `Any`.
    pub fn own_type_params_approximated(&mut self, t: TypeId, sym: SymbolId) -> Option<TypeId> {
        let mut bounded = false;
        let approximated = self.u.map_type(t, &mut |u, x| {
            let Type::Sym(None, s) = *u.types().get(x) else { return None };
            if u.symbol(s).owner != Some(sym) || u.symbol(s).kind != Kind::TypeParam {
                return None;
            }
            let info = u.info(s);
            let (nothing, any) = (u.builtins().nothing, u.builtins().any);
            let trivial = match *u.types().get(info) {
                Type::Bounds(lo, hi) => {
                    u.class_symbol(lo) == Some(nothing) && u.class_symbol(hi) == Some(any)
                }
                _ => false,
            };
            bounded |= !trivial;
            Some(u.intern_type(Type::Wildcard(None)))
        });
        (!bounded).then_some(approximated)
    }

    /// Whether `t` refers to a type parameter of method `sym`.
    pub fn mentions_own_type_params(&mut self, t: TypeId, sym: SymbolId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, x| {
            if let Type::Sym(None, s) = *u.types().get(x) {
                found |= u.symbol(s).owner == Some(sym) && u.symbol(s).kind == Kind::TypeParam;
            }
            None
        });
        found
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
