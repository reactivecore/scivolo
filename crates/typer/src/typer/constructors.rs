//! `new C` (dotty's `typedNew`) and the types of constructors and their calls.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::NodeId;
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
        let tycon = match *self.u.types().get(tpe) {
            Type::Applied(tc, _) => tc,
            _ => tpe,
        };
        if let Type::Sym(_, s) = *self.u.types().get(tycon)
            && self.u.symbol(s).kind == Kind::Type
            && !self.u.symbol(s).owner.is_some_and(|o| self.is_static_owner(o))
        {
            return self.unsupported(id, "new of a type alias of a non-static owner");
        }
        if let Type::Sym(_, s) = *self.u.types().get(tycon)
            && self.u.symbol(s).kind == Kind::Type
            && self.syms.flags.contains_key(&s)
            && let info = self.u.info(s)
            && matches!(self.u.types().get(info), Type::Lambda { .. })
        {
            return self.unsupported(id, "new of a parameterized type alias");
        }
        if let Some(c) = self.u.class_symbol(tpe)
            && !self.is_parent_call(c, ctx)
        {
            self.ensure_self(c);
            if self.u.given_self_type(c).is_some() {
                return self.unsupported(id, "new of a class not conforming to its self type");
            }
        }
        let erased = self.resolve_full_name("scala.compiletime.Erased", true);
        if let (Some(c), Some(erased)) = (self.u.class_symbol(tpe), erased)
            && self.u.derives_from(c, erased)
        {
            return self.unsupported(id, "new of an erased class");
        }
        if let Type::Sym(_, c) = *self.u.types().get(tpe)
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
        self.tpd.node(T::New { tpt }, span, Some(tpe), None)
    }

    /// `[Ps] =>> C[Ps]` for the type parameters `Ps` of class `c`, referenced by `class`.
    pub fn eta_expanded(&mut self, c: SymbolId, class: TypeId) -> Result<TypeId, &'static str> {
        let tparams = self.u.type_params(c);
        let prefix = match *self.u.types().get(class) {
            Type::Sym(Some(p), _) if !matches!(self.u.types().get(p), Type::Package(_)) => Some(p),
            _ => None,
        };
        let mut params = Vec::new();
        for &p in &tparams {
            self.complete(p);
            let info = match prefix {
                Some(prefix) => self.u.member_info(prefix, p),
                None => self.u.info(p),
            };
            let mut dependent = false;
            self.u.map_type(info, &mut |u, part| {
                dependent |= match *u.types().get(part) {
                    Type::Sym(_, s) => tparams.contains(&s),
                    Type::This(_) => prefix.is_some(),
                    _ => false,
                };
                None
            });
            if dependent {
                return Err("type parameter bounds referring to type parameters");
            }
            params.push((self.u.symbol(p).name.clone(), info));
        }
        let binder = self.u.fresh_binder();
        let args = (0..params.len()).map(|i| self.u.intern_type(Type::Param(binder, i as u32))).collect();
        let result = self.u.intern_type(Type::Applied(class, args));
        Ok(self.u.intern_type(Type::Lambda { binder, params, result }))
    }

    /// `info` of `s` if `s` is a constructor selected from `prefix`, its `Unit` result replaced by
    /// its class, as `prefix` refers to it, applied to its type parameters (dotty's
    /// `effectiveResultType`).
    pub fn effective_constructor_info(
        &mut self,
        s: SymbolId,
        info: TypeId,
        prefix: Option<TypeId>,
    ) -> TypeId {
        let sym = self.u.symbol(s);
        let Some(owner) = sym.owner.filter(|_| sym.name == "<init>" && sym.kind == Kind::Def) else {
            return info;
        };
        let tycon = prefix.map(|p| match *self.u.types().get(p) {
            Type::Applied(tc, _) => tc,
            _ => p,
        });
        let class = match tycon {
            Some(tc) if matches!(*self.u.types().get(tc), Type::Sym(_, c) if c == owner) => tc,
            _ => crate::defn::type_ref(self.u, owner),
        };
        match self.u.types().get(info).clone() {
            Type::Poly { binder, params, result } => {
                let args =
                    (0..params.len()).map(|i| self.u.intern_type(Type::Param(binder, i as u32))).collect();
                let class = self.u.intern_type(Type::Applied(class, args));
                let result = self.with_final_result(result, class);
                self.u.intern_type(Type::Poly { binder, params, result })
            }
            _ => self.with_final_result(info, class),
        }
    }

    /// Method type `t` with its final result, if `Unit`, replaced by `class`.
    fn with_final_result(&mut self, t: TypeId, class: TypeId) -> TypeId {
        match self.u.types().get(t).clone() {
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
        let names_owner = |u: &tasty::Universe, t| match *u.types().get(t) {
            Type::Sym(_, c) => Some(c) == owner,
            Type::Applied(c, _) => matches!(*u.types().get(c), Type::Sym(_, c) if Some(c) == owner),
            _ => false,
        };
        match owner {
            _ if names_owner(self.u, result) => result,
            Some(o) if !matches!(self.u.types().get(class), Type::Applied(..)) => {
                crate::defn::type_ref(self.u, o)
            }
            _ => class,
        }
    }
}
