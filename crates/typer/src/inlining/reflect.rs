// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The members of `quotes.reflect` macros may use, over the typer's trees, types and symbols
//! (dotty's `QuotesImpl.reflect`).

use crate::defn::type_ref;
use crate::tpd::{Build, TNodeId, Tpd};
use syntax::ast::Span;
use syntax::names::Names;
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
            let term_ref = match *self.u.ty(t) {
                Type::TermRef(..) => true,
                Type::Sym(_, s) => self.u.symbol(s).kind.is_term(),
                _ => false,
            };
            let Some(w) = term_ref.then(|| self.u.widen_once(t)).flatten().filter(|&w| w != t) else {
                return t;
            };
            t = self.u.by_name(w).unwrap_or(w);
        }
    }

    /// `TypeRepr.typeSymbol`: the symbol a type reference names, an alias's own; through the
    /// underlying type of other proxies; none for other types.
    pub fn type_symbol(&mut self, t: TypeId) -> Option<SymbolId> {
        match *self.u.ty(t) {
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
            Type::Lambda { binder, params, result } => {
                let types = self.u.types();
                let eta = match *types.get(result) {
                    Type::Applied(tycon, args)
                        if types.types_of(args).len() == types.params_of(params).len() =>
                    {
                        let own = (types.types_of(args))
                            .iter()
                            .enumerate()
                            .all(|(i, &a)| *types.get(a) == Type::Param(binder, i as u32));
                        own.then_some(tycon)
                    }
                    _ => None,
                };
                eta.and_then(|tycon| self.type_symbol(tycon))
            }
            Type::Annotated(inner, _) => self.type_symbol(inner),
            Type::This(c) => self.u.class_symbol(c),
            _ => None,
        }
    }

    /// `TypeRepr.termSymbol`: the symbol a term reference names, through annotations; none for
    /// class references, applied, constant and `this` types; a refusal for other types.
    pub fn term_symbol(&mut self, t: TypeId) -> Result<Option<SymbolId>, &'static str> {
        match *self.u.ty(t) {
            Type::Sym(_, s) if self.u.symbol(s).kind.is_term() => Ok(Some(s)),
            Type::TermRef(..) => Ok(self.u.resolve_ref(t)),
            Type::Annotated(inner, _) => self.term_symbol(inner),
            Type::Sym(_, s) if self.u.is_class(s) => Ok(None),
            Type::Applied(..) | Type::Constant(..) | Type::This(_) => Ok(None),
            _ => Err("TypeRepr.termSymbol of a type other than a reference, an applied or a constant type"),
        }
    }

    /// `TypeRepr.typeArgs`: the arguments of an applied type, none for other types.
    pub fn type_args(&mut self, t: TypeId) -> Vec<TypeId> {
        self.u.applied(t).map_or_else(Vec::new, |(_, args)| args.to_vec())
    }

    /// `Symbol.companionModule`: the object of class `s`, its constructor proxy object if it has
    /// none, or `s`'s own if it is a module class.
    pub fn companion_module(&mut self, s: SymbolId) -> Option<SymbolId> {
        match self.u.is_module_class(s) {
            true => self.u.source_module(s),
            false if self.u.is_class(s) => {
                let proxy = |u: &mut Universe| crate::typer::constructor_proxies::proxy_module(u, s);
                self.u.companion_module(s).or_else(|| proxy(self.u))
            }
            false => None,
        }
    }

    /// `Symbol.declaredMethod(name)`: the methods named `name` that class or object `s` declares.
    pub fn declared_methods(&mut self, s: SymbolId, name: &str) -> Vec<SymbolId> {
        let class = match self.u.is_module(s) {
            true => self.u.module_class(s),
            false => Some(s),
        };
        let decls = class.map(|c| self.u.decls_named(c, &tasty::decode(name))).unwrap_or_default();
        decls.into_iter().filter(|&d| self.u.symbol(d).kind == Kind::Def).collect()
    }

    /// `Symbol.typeRef`, `Symbol.termRef`: the type of a reference to `s`.
    pub fn symbol_ref(&mut self, s: SymbolId) -> TypeId {
        if self.u.symbol(s).kind == Kind::Package {
            return self.u.intern_type(Type::Package(s));
        }
        type_ref(self.u, s)
    }

    /// `TypeRepr.select(s)`: the reference to member `s` of `prefix`.
    pub fn select_type(&mut self, prefix: TypeId, s: SymbolId) -> TypeId {
        crate::defn::ref_type(self.u, Some(prefix), s)
    }

    /// `TermRef(qual, name)`: the reference to the only term member `name` of `qual`.
    pub fn term_ref(&mut self, qual: TypeId, name: &str) -> Result<TypeId, &'static str> {
        let found = match *self.u.ty(crate::defn::package_ref(self.u, qual)) {
            Type::Package(p) => self.u.decls_named(p, name),
            _ => self.u.member_of_type(qual, name),
        };
        let terms: Vec<SymbolId> = found.into_iter().filter(|&s| self.u.symbol(s).kind.is_term()).collect();
        let [s] = terms[..] else { return Err("TermRef.apply of no or an overloaded term member") };
        Ok(crate::defn::ref_type(self.u, Some(qual), s))
    }

    /// `Ref(s)` of a static or local term `s`, or `C.this.s` of a member of class `C`.
    pub fn ref_(&mut self, s: SymbolId) -> Result<TNodeId, &'static str> {
        let span = self.span;
        let owner = self.u.symbol(s).owner.ok_or("Ref of a symbol without an owner")?;
        if !self.u.symbol(s).kind.is_term() {
            return Err("Ref of a type");
        }
        if self.u.is_static_symbol(s) || self.u.symbol(owner).kind.is_term() {
            let tpe = type_ref(self.u, s);
            return Ok(self.build()?.ident(s, tpe, span));
        }
        if self.u.symbol(owner).kind != Kind::Class {
            return Err("Ref of a symbol other than a static term, a local or a class member");
        }
        let class = type_ref(self.u, owner);
        let mut b = self.build()?;
        let this = b.this(class, span);
        Ok(b.select(this, s, span))
    }

    /// `Term.select(m)`.
    pub fn select(&mut self, qual: TNodeId, m: SymbolId) -> Result<TNodeId, &'static str> {
        let span = self.span;
        Ok(self.build()?.select(qual, m, span))
    }
}
