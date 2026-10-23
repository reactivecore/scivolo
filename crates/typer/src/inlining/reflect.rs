// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The members of `quotes.reflect` macros may use, over the typer's trees, types and symbols
//! (dotty's `QuotesImpl.reflect`).

use crate::defn::type_ref;
use crate::tpd::{Build, TNodeId, Tpd, Tree as T};
use crate::typer::type_apply::instantiate_poly;
use syntax::ast::Span;
use syntax::names::Names;
use tasty::tags::CLASSconst;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

pub struct Reflect<'a> {
    pub u: &'a mut Universe,
    pub tpd: &'a mut Tpd,
    pub names: &'a mut Names,
    /// The span of the trees it builds: the macro expansion's (dotty's `withDefaultPos`).
    pub span: Span,
}

impl Reflect<'_> {
    pub fn build(&mut self) -> Result<Build<'_>, &'static str> {
        let scala = self.u.package("scala").ok_or("a library without package scala")?;
        let unit = self.u.decl(scala, "Unit").ok().and_then(|d| d.into_iter().find(|&s| self.u.is_class(s)));
        Ok(Build::new(self.tpd, self.u, self.names, unit.ok_or("a library without scala.Unit")?))
    }

    /// `TypeRepr.widenTermRefByName` (dotty's `widenTermRefExpr`): `t` with term references
    /// widened, a by-name type's result for a by-name type a term reference widens to.
    pub fn widen_term_ref_by_name(&mut self, mut t: TypeId) -> TypeId {
        loop {
            let term_ref = match *self.u.types().get(t) {
                Type::TermRef(..) => true,
                Type::Sym(_, s) => self.u.symbol(s).kind.is_term(),
                _ => false,
            };
            let Some(w) = term_ref.then(|| self.u.widen_once(t)).flatten().filter(|&w| w != t) else {
                return t;
            };
            t = match *self.u.types().get(w) {
                Type::ByName(r) => r,
                _ => w,
            };
        }
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

    /// `Symbol.typeRef`, `Symbol.termRef`: the type of a reference to `s`.
    pub fn symbol_ref(&mut self, s: SymbolId) -> TypeId {
        type_ref(self.u, s)
    }

    /// `TermRef(qual, name)`: the reference to the only term member `name` of `qual`.
    pub fn term_ref(&mut self, qual: TypeId, name: &str) -> Result<TypeId, &'static str> {
        let found = match *self.u.types().get(qual) {
            Type::Package(p) => self.u.decl(p, name).unwrap_or_default(),
            _ => self.u.member_of_type(qual, name),
        };
        let terms: Vec<SymbolId> = found.into_iter().filter(|&s| self.u.symbol(s).kind.is_term()).collect();
        let [s] = terms[..] else { return Err("TermRef.apply of no or an overloaded term member") };
        Ok(crate::defn::ref_type(self.u, Some(qual), s))
    }

    /// `Ref(s)` of a static object or value `s`.
    pub fn ref_(&mut self, s: SymbolId) -> Result<TNodeId, &'static str> {
        let span = self.span;
        if !self.u.symbol(s).kind.is_term() || !self.u.is_static_symbol(s) {
            return Err("Ref of a symbol other than a static term");
        }
        let tpe = type_ref(self.u, s);
        Ok(self.build()?.ident(s, tpe, span))
    }

    /// `Term.select(m)`.
    pub fn select(&mut self, qual: TNodeId, m: SymbolId) -> Result<TNodeId, &'static str> {
        let span = self.span;
        Ok(self.build()?.select(qual, m, span))
    }

    /// `Apply(fun, args)` (dotty's `tpd.Apply`), of the result of `fun`'s method type for the
    /// arguments' types.
    pub fn apply(&mut self, fun: TNodeId, args: &[TNodeId]) -> Result<TNodeId, &'static str> {
        let span = self.span;
        let info = self.u.widen(self.tpd.tpe(fun));
        let Type::Method { binder, params, result, .. } = self.u.types().get(info).clone() else {
            return Err("Apply of a term that takes no arguments");
        };
        if params.len() != args.len() {
            return Err("Apply with another number of arguments than parameters");
        }
        let names: Vec<String> = params.into_iter().map(|(n, _)| n).collect();
        let tpes: Vec<TypeId> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
        let meth = self.tpd.sym(self.tpd.meth_part(fun));
        let tpe =
            crate::typer::dependent_params::applied_result(self.u, result, (binder, &names), meth, &tpes);
        Ok(self.build()?.apply(fun, args, tpe, span))
    }

    /// `Term.appliedToTypes(args)`.
    pub fn applied_to_types(&mut self, fun: TNodeId, args: &[TypeId]) -> Result<TNodeId, &'static str> {
        let span = self.span;
        let refused = "appliedToTypes of a term that takes no such type arguments";
        if let (Some(f), &[arg]) = (self.tpd.sym(fun), args)
            && self.u.full_name(f) == "scala.Predef$.classOf"
        {
            let tpe = self.u.intern_type(Type::Constant(CLASSconst, i64::from(arg.0)));
            return Ok(self.tpd.node(T::ClassLiteral { tpe: arg }, span, Some(tpe), None));
        }
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
            .map(|&a| self.tpd.node(T::TypeTree { inferred: false }, span, Some(a), None))
            .collect();
        Ok(self.build()?.type_apply(fun, &tpts, tpe, span))
    }
}
