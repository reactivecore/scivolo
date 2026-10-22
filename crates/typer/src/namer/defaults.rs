// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! The result types of default getters, `m$default$N`: the type of `m`'s parameter `N` (dotty's
//! `defaultParamType` in `Namer.inferredResultType`).

use crate::typer::Typer;
use syntax::ast::flags as f;
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The type of the parameter default getter `getter` supplies, if it is one, and that type
    /// with references to the method's value parameters approximated (dotty's
    /// `expectedDefaultArgType`); references to the method's type parameters become references
    /// to the getter's same-named ones; none unless one method of the name has defaults, a
    /// retracted synthetic one counted for a getter of the user, which dotty completes first.
    pub fn default_param_type(&mut self, getter: SymbolId) -> Option<(TypeId, TypeId)> {
        let name = self.u.symbol(getter).name.clone();
        let (method, index) = name.rsplit_once("$default$")?;
        let index: usize = index.parse().ok()?;
        let owner = self.u.symbol(getter).owner?;
        let mut candidates: Vec<SymbolId> = self.u.symbol(owner).decls.clone();
        if !self.syms.flags.get(&getter).is_some_and(|fl| fl.is(f::SYNTHETIC)) {
            candidates.extend(self.syms.retracted.iter().filter(|&&d| self.u.symbol(d).owner == Some(owner)));
        }
        candidates.retain(|&d| self.u.symbol(d).kind == Kind::Def && self.u.symbol(d).name == method);
        candidates.extend(self.constructor_of_companion(owner, method));
        let mut with_defaults = Vec::new();
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
            if params.iter().any(|p| self.syms.flags.get(p).is_some_and(|fl| fl.is(f::HAS_DEFAULT))) {
                with_defaults.push((m, params));
            }
        }
        let [(m, params)] = &with_defaults[..] else { return None };
        let &param = params.get(index.checked_sub(1)?)?;
        let info = self.u.info(param);
        let exact = self.with_getter_type_params(info, *m, getter);
        let approx = value_params_approximated(self.u, exact, *m);
        let pt = if approx != exact && self.is_context_function_type(approx) { exact } else { approx };
        Some((exact, pt))
    }

    /// `default_param_type` of `getter` with by-name types replaced by their results (dotty's
    /// `widenExpr`), and whether it was by-name.
    pub fn default_param_proto(&mut self, getter: SymbolId) -> (Option<(TypeId, TypeId)>, bool) {
        let Some((exact, pt)) = self.default_param_type(getter) else { return (None, false) };
        let types = self.u.types();
        match (types.get(exact), types.get(pt)) {
            (&Type::ByName(e), &Type::ByName(p)) => (Some((e, p)), true),
            _ => (Some((exact, pt)), false),
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

    /// The constructors of the class `owner`, an object, accompanies, for `<init>`'s default
    /// getters, which live in the companion.
    fn constructor_of_companion(&mut self, owner: SymbolId, method: &str) -> Vec<SymbolId> {
        let primary = if method == "<init>" { self.companion_constructor(owner) } else { None };
        let class = primary.and_then(|p| self.u.symbol(p).owner);
        class.map(|c| self.u.decl(c, "<init>").unwrap_or_default()).unwrap_or_default()
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

/// `t` with references to the value parameters of `method` made wildcards bounded by their types,
/// and selections on such a wildcard made wildcards (dotty's `wildApprox` of a `TermParamRef`).
fn value_params_approximated(u: &mut tasty::Universe, t: TypeId, method: SymbolId) -> TypeId {
    u.map_type(t, &mut |u, x| match *u.types().get(x) {
        Type::Sym(None, s) if u.symbol(s).owner == Some(method) && u.symbol(s).kind == Kind::Param => {
            let info = u.info(s);
            let hi = value_params_approximated(u, info, method);
            let bounds = u.intern_type(Type::Bounds(tasty::NOTHING, hi));
            Some(u.intern_type(Type::Wildcard(Some(bounds))))
        }
        Type::TypeRef(p, _) | Type::TermRef(p, _) | Type::Sym(Some(p), _) => {
            let p1 = value_params_approximated(u, p, method);
            matches!(u.types().get(p1), Type::Wildcard(_)).then(|| u.intern_type(Type::Wildcard(None)))
        }
        _ => None,
    })
}
