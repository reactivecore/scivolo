//! Types the desugaring leaves to the typer (dotty's `DerivedTypeTree`s): a class parameter's
//! copy in the class body, a setter's parameter, a reference to a class.

use super::Typer;
use crate::ctx::Ctx;
use crate::defn::type_ref;
use crate::tpd::TNodeId;
use syntax::ast::tree::DerivedKind;
use syntax::ast::{NodeId, TName};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn typed_derived(&mut self, id: NodeId, kind: DerivedKind, watched: Option<TName>, ctx: Ctx) -> TNodeId {
        let Some(watched) = watched else { return self.unsupported(id, "derived type without a source") };
        let text = self.text(watched.name).to_owned();
        let tpe = match kind {
            DerivedKind::FromParam => self.derived_from_param(&text, watched.is_type, ctx),
            DerivedKind::SetterParam => self.setter_param_type(&text, ctx),
            DerivedKind::TypeRef => self.class_named(&text, ctx).map(|c| type_ref(self.u, c)),
            DerivedKind::TermRef => None,
        };
        let Some(tpe) = tpe else { return self.unsupported(id, "derived type") };
        let span = self.ast.span(id);
        self.type_tree(tpe, span, false)
    }

    /// The info of the primary constructor's parameter `name` of the class of `ctx`, its
    /// references to the constructor's other parameters made to the class's parameters
    /// (dotty's `DerivedFromParamTree.derivedTree`).
    fn derived_from_param(&mut self, name: &str, is_type: bool, ctx: Ctx) -> Option<TypeId> {
        let cls = self.enclosing_class(ctx.owner)?;
        let ctor = *self.syms.primary_ctors.get(&cls)?;
        self.complete(ctor);
        let decls = self.u.symbol(ctor).decls.clone();
        let original = decls.into_iter().find(|&p| self.u.symbol(p).name == name && self.is_type_symbol(p) == is_type)?;
        let info = self.u.info(original);
        let this = {
            let class = type_ref(self.u, cls);
            self.u.intern_type(Type::This(class))
        };
        let locals: Vec<(String, bool, SymbolId)> = self
            .u
            .symbol(cls)
            .decls
            .iter()
            .map(|&d| (self.u.symbol(d).name.clone(), self.is_type_symbol(d), d))
            .filter(|&(_, _, d)| self.syms.flags.get(&d).is_some_and(|f| f.is_one_of(syntax::ast::flags::PARAM.or(syntax::ast::flags::PARAM_ACCESSOR))))
            .collect();
        let mut failed = false;
        let relocated = self.u.map_type(info, &mut |u, t| {
            let Type::Sym(_, s) = *u.types().get(t) else { return None };
            if u.symbol(s).owner != Some(ctor) {
                return None;
            }
            let is_type = matches!(u.symbol(s).kind, Kind::TypeParam | Kind::Type | Kind::Class);
            match locals.iter().find(|(n, ty, _)| *n == u.symbol(s).name && *ty == is_type) {
                Some(&(_, _, local)) => Some(u.intern_type(Type::Sym(Some(this), local))),
                None => {
                    failed = true;
                    None
                }
            }
        });
        (!failed).then_some(relocated)
    }

    /// The type of the variable `name` whose setter's parameter is being typed in `ctx`.
    fn setter_param_type(&mut self, name: &str, ctx: Ctx) -> Option<TypeId> {
        let setter = ctx.owner;
        let owner = self.u.symbol(setter).owner?;
        let decls = self.u.symbol(owner).decls.clone();
        let var = decls.into_iter().find(|&d| self.u.symbol(d).kind == Kind::Val && self.u.symbol(d).name == name)?;
        self.complete(var);
        Some(self.u.info(var))
    }

    /// The class named `name` defined in the owner of `ctx` or around it.
    fn class_named(&mut self, name: &str, ctx: Ctx) -> Option<SymbolId> {
        match self.find_ref(name, true, ctx) {
            Ok(Some(found)) => found.syms.into_iter().find(|&s| self.u.symbol(s).kind == Kind::Class),
            _ => None,
        }
    }
}
