//! The members of `quotes.reflect` macros may use, over the typer's trees, types and symbols
//! (dotty's `QuotesImpl.reflect`).

use crate::defn::type_ref;
use crate::tpd::{Build, TNodeId, Tpd, Tree as T};
use crate::typer::type_apply::instantiate_poly;
use syntax::ast::Span;
use syntax::ast::constant::Constant;
use syntax::names::Names;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

pub struct Reflect<'a> {
    pub u: &'a mut Universe,
    pub tpd: &'a mut Tpd,
    pub names: &'a mut Names,
}

impl Reflect<'_> {
    fn build(&mut self) -> Result<Build<'_>, &'static str> {
        let scala = self.u.package("scala").ok_or("a library without package scala")?;
        let unit = self.u.decl(scala, "Unit").ok().and_then(|d| d.into_iter().find(|&s| self.u.is_class(s)));
        Ok(Build::new(self.tpd, self.u, self.names, unit.ok_or("a library without scala.Unit")?))
    }

    /// `Term.underlying` (dotty's `underlyingArgument` at the top of `t`): `t` without the
    /// inlined trees without bindings, empty blocks and argument names around it, a reference to
    /// a local value replaced by the value's right-hand side.
    pub fn underlying(&self, t: TNodeId) -> TNodeId {
        match *self.tpd.tree(t) {
            T::Inlined { bindings, expansion, .. } if bindings.is_empty() => self.underlying(expansion),
            T::Block { stats, expr } if stats.is_empty() => self.underlying(expr),
            T::NamedArg { arg, .. } => self.underlying(arg),
            T::Ident { .. } => match self.tpd.sym(t).and_then(|s| self.local_rhs(s)) {
                Some(rhs) => self.underlying(rhs),
                None => t,
            },
            _ => t,
        }
    }

    /// The right-hand side of the definition of `s` if it is a value owned by a term.
    fn local_rhs(&self, s: SymbolId) -> Option<TNodeId> {
        let sym = self.u.symbol(s);
        let local = sym.owner.is_some_and(|o| self.u.symbol(o).kind.is_term());
        if sym.kind != Kind::Val || !local {
            return None;
        }
        (0..self.tpd.len()).rev().map(TNodeId::new).find_map(|d| match *self.tpd.tree(d) {
            T::ValDef { rhs: Some(rhs), .. } if self.tpd.sym(d) == Some(s) => Some(rhs),
            _ => None,
        })
    }

    /// `Literal.constant`, if `t` is a literal.
    pub fn constant(&self, t: TNodeId) -> Option<Constant> {
        match self.tpd.tree(t) {
            T::Literal { value } => Some(value.clone()),
            _ => None,
        }
    }

    /// `Literal(constant)`.
    pub fn literal(&mut self, constant: Constant) -> Result<TNodeId, &'static str> {
        Ok(self.build()?.literal(constant, Span::NONE))
    }

    /// `TypeRepr.typeSymbol`: the symbol a type reference names, an alias's own; through the
    /// underlying type of other proxies; none for other types.
    pub fn type_symbol(&mut self, t: TypeId) -> Option<SymbolId> {
        match *self.u.types().get(t) {
            Type::Sym(_, s) if self.u.symbol(s).kind.is_term() => {
                let widened = self.u.widen_once(t)?;
                self.type_symbol(widened)
            }
            Type::TermRef(..) => {
                let widened = self.u.widen_once(t)?;
                self.type_symbol(widened)
            }
            Type::Sym(_, s) => Some(s),
            Type::TypeRef(..) => self.u.resolve_ref(t),
            Type::Applied(tycon, _) => self.type_symbol(tycon),
            Type::Annotated(inner, _) => self.type_symbol(inner),
            Type::This(c) => self.u.class_symbol(c),
            _ => None,
        }
    }

    /// `TypeRepr.typeArgs`: the arguments of an applied type, none for other types.
    pub fn type_args(&mut self, t: TypeId) -> Vec<TypeId> {
        match self.u.types().get(t) {
            Type::Applied(_, args) => args.clone(),
            _ => Vec::new(),
        }
    }

    /// `Symbol.companionModule`: the object of class `s`, or `s`'s own if it is a module class.
    pub fn companion_module(&mut self, s: SymbolId) -> Option<SymbolId> {
        match self.u.is_module_class(s) {
            true => self.u.source_module(s),
            false if self.u.is_class(s) => self.u.companion_module(s),
            false => None,
        }
    }

    /// `Symbol.declaredMethod(name)`: the methods named `name` that class or object `s` declares.
    pub fn declared_methods(&mut self, s: SymbolId, name: &str) -> Vec<SymbolId> {
        let class = match self.u.is_module(s) {
            true => self.u.module_class(s),
            false => Some(s),
        };
        let decls = class.and_then(|c| self.u.decl(c, &tasty::decode(name)).ok()).unwrap_or_default();
        decls.into_iter().filter(|&d| self.u.symbol(d).kind == Kind::Def).collect()
    }

    /// `Ref(s)` of a static object or value `s`.
    pub fn ref_(&mut self, s: SymbolId) -> Result<TNodeId, &'static str> {
        if !self.u.symbol(s).kind.is_term() || !self.u.is_static_symbol(s) {
            return Err("Ref of a symbol other than a static term");
        }
        let tpe = type_ref(self.u, s);
        Ok(self.build()?.ident(s, tpe, Span::NONE))
    }

    /// `Term.select(m)`.
    pub fn select(&mut self, qual: TNodeId, m: SymbolId) -> Result<TNodeId, &'static str> {
        Ok(self.build()?.select(qual, m, Span::NONE))
    }

    /// `Term.appliedToTypes(args)`.
    pub fn applied_to_types(&mut self, fun: TNodeId, args: &[TypeId]) -> Result<TNodeId, &'static str> {
        let refused = "appliedToTypes of a term that takes no such type arguments";
        let info = self.u.widen(self.tpd.tpe(fun));
        let Type::Poly { binder, params, result } = self.u.types().get(info).clone() else {
            return Err(refused);
        };
        if params.len() != args.len() {
            return Err(refused);
        }
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        let tpe = instantiate_poly(self.u, result, (binder, &names), args, self.tpd.sym(fun));
        let tpts: Vec<TNodeId> = args
            .iter()
            .map(|&a| self.tpd.node(T::TypeTree { inferred: false }, Span::NONE, Some(a), None))
            .collect();
        Ok(self.build()?.type_apply(fun, &tpts, tpe, Span::NONE))
    }
}
