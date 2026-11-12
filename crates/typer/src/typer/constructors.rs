// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! `new C` (dotty's `typedNew`) and the types of constructors and their calls.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use names::nme;
use syntax::ast::NodeId;
use tasty::tags::{ABSTRACT, TRAIT};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    pub fn typed_new(&mut self, id: NodeId, tpt: NodeId, ctx: Ctx) -> TNodeId {
        let tpt = self.typed_type(tpt, ctx);
        let tpe = self.tpd.tpe(tpt);
        if let T::Select { qualifier, .. } = *self.tpd.tree(tpt)
            && !self.is_stable_path(qualifier)
        {
            return self.unsupported(id, "new with an unstable prefix");
        }
        if let Some(s) = self.u.sym_of(tpe)
            && self.u.symbol(s).kind == Kind::Type
            && self.syms.flags.contains_key(&s)
            && let info = self.u.info(s)
            && self.u.lambda(info).is_some()
            && !self.parent_news.contains(&id)
        {
            return self.unsupported(id, "new of a parameterized type alias without type arguments");
        }
        if let Some(c) = self.u.class_symbol(tpe)
            && !self.parent_news.contains(&id)
        {
            let flags = self.u.symbol(c).flags;
            let java_annotation =
                self.annotation_news.contains(&id) && self.u.classfile_retention(c).is_some();
            if (flags.has(TRAIT) || flags.has(ABSTRACT)) && !java_annotation {
                let name = self.u.symbol(c).name.clone();
                let span = self.tpd.span(tpt);
                self.error_at(span, super::errors::cannot_instantiate(&name, flags.has(TRAIT)));
            }
            self.ensure_self(c);
            if self.u.given_self_type(c).is_some() {
                return self.unsupported(id, "new of a class not conforming to its self type");
            }
        }
        if let Some(c) = self.u.sym_of(tpe)
            && !self.u.type_params(c).is_empty()
        {
            match self.eta_expanded(c, tpe) {
                Ok(lambda) => {
                    let node = self.tpd.get_mut(tpt);
                    (node.tpe, node.sym) = (Some(lambda), None);
                }
                Err(what) => return self.unsupported(id, what),
            }
        }
        let span = self.ast.span(id);
        let tpe = self.eta_expansion_tycon(tpe).unwrap_or(tpe);
        self.tpd.node(T::New { tpt }, span, Some(tpe), None)
    }

    /// `C` if `t` dealiases to `[Ps] =>> C[Ps]` (dotty's `typeOfNew`).
    fn eta_expansion_tycon(&mut self, t: TypeId) -> Option<TypeId> {
        let dealiased = self.u.dealias(t);
        let lambda = self.u.lambda(dealiased)?;
        let n = lambda.params.len();
        let (tycon, args) = self.u.applied(lambda.result)?;
        let own = args.len() == n
            && args.iter().enumerate().all(|(i, &a)| self.u.param_ref(a) == Some((lambda.binder, i as u32)));
        own.then_some(tycon)
    }

    /// `[Ps] =>> C[Ps]` for the type parameters `Ps` of class `c`, referenced by `class`.
    pub fn eta_expanded(&mut self, c: SymbolId, class: TypeId) -> Result<TypeId, &'static str> {
        let tparams = self.u.type_params(c);
        let prefix =
            self.u.sym_ref(class).and_then(|(p, _)| p).filter(|&p| !matches!(self.u.ty(p), Type::Package(_)));
        let binder = self.u.fresh_binder();
        let mut params = Vec::new();
        for &p in &tparams {
            self.complete(p);
            let info = match prefix {
                Some(prefix) => self.u.member_info(prefix, p),
                None => self.u.info(p),
            };
            let info = self.u.map_type(info, &mut |u, part| {
                let (q, s) = u.sym_ref(part)?;
                let i = tparams.iter().position(|&p| p == s)?;
                let own = match q.map(|q| u.this_of(q)) {
                    None => true,
                    Some(Some(t)) => u.class_symbol(t) == Some(c),
                    Some(None) => false,
                };
                own.then(|| u.intern_type(Type::Param(binder, i as u32)))
            });
            let mut dependent = false;
            self.u.map_type(info, &mut |u, part| {
                dependent |= match *u.ty(part) {
                    Type::Sym(_, s) => tparams.contains(&s),
                    Type::This(t) => prefix.is_some() && u.class_symbol(t) == Some(c),
                    _ => false,
                };
                None
            });
            if dependent {
                return Err("type parameter bounds referring to type parameters");
            }
            params.push((self.u.interned_name(p), info));
        }
        let args: Vec<_> =
            (0..params.len()).map(|i| self.u.intern_type(Type::Param(binder, i as u32))).collect();
        let result = self.u.intern_applied(class, &args);
        Ok(self.u.intern_lambda(binder, &params, result))
    }

    /// `info` of `s` if `s` is a constructor selected from `prefix`, its `Unit` result replaced by
    /// its class, as `prefix` refers to it unless the class is static or `prefix` a package path,
    /// applied to its type parameters (dotty's `effectiveResultType`).
    pub fn effective_constructor_info(
        &mut self,
        s: SymbolId,
        info: TypeId,
        prefix: Option<TypeId>,
    ) -> TypeId {
        let sym = self.u.symbol(s);
        let Some(owner) = sym.owner.filter(|_| sym.name == nme::CONSTRUCTOR && sym.kind == Kind::Def) else {
            return info;
        };
        let tycon = prefix.map(|p| self.u.applied(p).map_or(p, |(tc, _)| tc));
        let package = |u: &tasty::Universe, p: Option<TypeId>| {
            p.is_some_and(|p| matches!(u.ty(p), Type::Package(_) | Type::PackageName(_)))
        };
        let fixed = self.u.is_static_symbol(owner) && !self.u.sees_opaques(owner);
        let class = match tycon {
            Some(tc) if !fixed && self.u.sym_ref(tc).is_some_and(|(p, c)| c == owner && !package(self.u, p)) => tc,
            _ => crate::defn::type_ref(self.u, owner),
        };
        match *self.u.ty(info) {
            Type::Poly { binder, params, result } => {
                let arity = self.u.params_of(params).len();
                let args: Vec<_> =
                    (0..arity).map(|i| self.u.intern_type(Type::Param(binder, i as u32))).collect();
                let class = self.u.intern_applied(class, &args);
                let result = self.with_final_result(result, class);
                self.u.intern_type(Type::Poly { binder, params, result })
            }
            _ => self.with_final_result(info, class),
        }
    }

    /// Method type `t` with its final result, if `Unit`, replaced by `class`.
    fn with_final_result(&mut self, t: TypeId, class: TypeId) -> TypeId {
        let node = *self.u.ty(t);
        match node {
            Type::Method { binder, params, result, implicit, contextual } => {
                let result = self.with_final_result(result, class);
                self.u.intern_type(Type::Method { binder, params, result, implicit, contextual })
            }
            _ if self.u.class_symbol(t) == Some(self.defn.unit) => class,
            _ => t,
        }
    }

    /// The type of a call of constructor `ctor` on `new` of type `class`, whose method type gives
    /// `result`: `result` if it names the constructor's class, else the class itself unless
    /// `class` applies it to arguments.
    pub fn constructor_result(&mut self, ctor: SymbolId, class: TypeId, result: TypeId) -> TypeId {
        let owner = self.u.symbol(ctor).owner;
        let names_owner = |u: &tasty::Universe, t| {
            u.sym_of(u.applied(t).map_or(t, |(c, _)| c)).is_some_and(|c| Some(c) == owner)
        };
        match owner {
            _ if names_owner(self.u, result) => result,
            Some(o) if self.u.applied(class).is_none() => crate::defn::type_ref(self.u, o),
            _ => class,
        }
    }

    /// The primary constructor of the companion class of object class `module`.
    pub fn companion_constructor(&mut self, module: SymbolId) -> Option<SymbolId> {
        let name = tasty::module_name(&self.u.symbol(module).name)?.to_owned();
        let owner = self.u.symbol(module).owner?;
        let decls = self.u.symbol(owner).decls.clone();
        let companion = decls
            .into_iter()
            .find(|&d| self.u.symbol(d).kind == Kind::Class && self.u.symbol(d).name == name)?;
        self.complete(companion);
        self.syms.primary_ctors.get(&companion).copied()
    }
}
