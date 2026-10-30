// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuoteMatcher.scala
//! The closure a higher-order hole's open term is made (dotty's `MatchResult.toExpr` of an
//! `OpenTree`), polymorphic over the type parameters the hole captures.

use super::super::copy::TreeTypeMap;
use super::super::reflect_beta::NoHost;
use super::super::{Inlining, Reflect};
use super::Captured;
use crate::tpd::{TNodeId, Tree as T};
use rustc_hash::{FxHashMap, FxHashSet};
use syntax::ast::TName;
use syntax::names::FreshNames;
use tasty::tags::SYNTHETIC;
use tasty::{ANY, Flags, Kind, NOTHING, Symbol, SymbolId, Type, TypeId};

impl Reflect<'_> {
    /// The closure `[X0, ...] => (params) => t` owned by `owner`, its parameters, named as
    /// `params` name them, standing for the scrutinee's symbols there, its type parameters for
    /// the scrutinee's type parameters `tparams`.
    pub fn open_closure(
        &mut self,
        owner: SymbolId,
        t: TNodeId,
        (params, tparams): (&[Captured], &[SymbolId]),
    ) -> Result<TNodeId, &'static str> {
        let span = self.tpd.span(t);
        let symbol = |name: &str, kind, owner| Symbol {
            name: name.to_owned(),
            kind,
            owner: Some(owner),
            flags: Flags::default().with(SYNTHETIC),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        };
        let anonfun = self.u.define(symbol("$anonfun", Kind::Def, owner));
        let (mut syms, mut type_params, mut tparam_trees) = (FxHashMap::default(), Vec::new(), Vec::new());
        let bounds = self.u.intern_type(Type::Bounds(NOTHING, ANY));
        for (i, &old) in tparams.iter().enumerate() {
            let name = format!("X{i}");
            let p = self.u.define(symbol(&name, Kind::TypeParam, anonfun));
            self.u.set_info(p, bounds);
            syms.insert(old, p);
            type_params.push(p);
            let rhs = self.type_tree(bounds);
            let name = TName { name: self.names.intern(&name), is_type: true };
            tparam_trees.push(self.tpd.node(T::TypeDef { name, rhs }, span, None, Some(p)));
        }
        let (mut types, mut param_trees, mut methods) = (Vec::new(), Vec::new(), Vec::new());
        for c in params {
            let p = self.u.define(symbol(&c.name, Kind::Param, anonfun));
            let tpe = match c.eta {
                Some(function) => {
                    methods.push(p);
                    function
                }
                None => {
                    let info = self.u.info(c.sym);
                    self.u.widen(info)
                }
            };
            let tpe = self.with_syms(tpe, &syms);
            self.u.set_info(p, tpe);
            syms.insert(c.sym, p);
            types.push(tpe);
            let tpt = self.type_tree(tpe);
            param_trees.push(self.build()?.val_def(p, tpt, None, span));
        }
        let result = self.tpd.tpe(t);
        let result = self.u.widen(result);
        let result = self.with_syms(result, &syms);
        let binder = self.u.fresh_binder();
        let method_params = params.iter().map(|c| c.name.clone()).zip(types.iter().copied()).collect();
        let info = Type::Method { binder, params: method_params, result, implicit: false, contextual: false };
        let info = self.u.intern_type(info);
        let (info, fun_tpe) = match type_params.is_empty() {
            true => {
                let class = self.u.class_ref(&format!("scala.Function{}", params.len()));
                types.push(result);
                (info, self.u.intern_type(Type::Applied(class, types)))
            }
            false => self.poly_function(&type_params, info),
        };
        self.u.set_info(anonfun, info);
        let body = self.copied(t, syms)?;
        self.reown(body, anonfun);
        self.applies_through(body, &methods)?;
        let tpt = self.type_tree(result);
        let clauses: Vec<&[TNodeId]> = match tparam_trees.is_empty() {
            true => vec![&param_trees],
            false => vec![&tparam_trees, &param_trees],
        };
        let def = self.build()?.def_def(anonfun, &clauses, tpt, Some(body), span);
        let meth_tpe = self.u.intern_type(Type::Sym(None, anonfun));
        let meth = self.build()?.ident(anonfun, meth_tpe, span);
        let env = self.tpd.list(&[]);
        let closure = self.tpd.node(T::Closure { env, meth, tpt: None }, span, Some(fun_tpe), None);
        let stats = self.tpd.list(&[def]);
        Ok(self.build()?.block(stats, closure, span))
    }

    /// `t` with the references to the symbols of `syms` replaced by references to those they map to.
    fn with_syms(&mut self, t: TypeId, syms: &FxHashMap<SymbolId, SymbolId>) -> TypeId {
        self.u.map_type(t, &mut |u, part| match *u.types().get(part) {
            Type::Sym(None, s) => syms.get(&s).map(|&n| u.intern_type(Type::Sym(None, n))),
            _ => None,
        })
    }

    /// The polymorphic method type over `tparams` of `method` and the `PolyFunction` of it.
    fn poly_function(&mut self, tparams: &[SymbolId], method: TypeId) -> (TypeId, TypeId) {
        let binder = self.u.fresh_binder();
        let result = self.u.map_type(method, &mut |u, part| match *u.types().get(part) {
            Type::Sym(None, s) => {
                let i = tparams.iter().position(|&p| p == s)?;
                Some(u.intern_type(Type::Param(binder, i as u32)))
            }
            _ => None,
        });
        let bounds = self.u.intern_type(Type::Bounds(NOTHING, ANY));
        let params = tparams.iter().map(|&p| (self.u.symbol(p).name.clone(), bounds)).collect();
        let info = self.u.intern_type(Type::Poly { binder, params, result });
        let parent = self.u.class_ref("scala.PolyFunction");
        (info, self.u.intern_type(Type::Refined(parent, "apply".into(), info)))
    }

    /// A copy of `t` with the references to the symbols of `syms` replaced by references to
    /// those they map to.
    fn copied(&mut self, t: TNodeId, syms: FxHashMap<SymbolId, SymbolId>) -> Result<TNodeId, &'static str> {
        let mut fresh = FreshNames::default();
        let mut host = NoHost;
        let mut i =
            Inlining::new((&mut *self.u, &mut *self.tpd), (&mut *self.names, &mut fresh), &mut host, false);
        let mut m = TreeTypeMap { syms, ..TreeTypeMap::default() };
        i.copy(&mut m, t, &mut |_, _, _| None).map_err(|(_, what)| what)
    }

    /// Gives the outermost definitions in `t` owner `owner`.
    fn reown(&mut self, t: TNodeId, owner: SymbolId) {
        let (mut todo, mut defs) = (vec![t], Vec::new());
        while let Some(n) = todo.pop() {
            if matches!(self.tpd.tree(n), T::DefDef { .. } | T::ValDef { .. } | T::Bind { .. }) {
                defs.extend(self.tpd.sym(n));
            }
            todo.extend(self.tpd.children(n));
        }
        let defined: FxHashSet<SymbolId> = defs.iter().copied().collect();
        for s in defs {
            if self.u.symbol(s).owner.is_some_and(|o| !defined.contains(&o)) {
                self.u.set_owner(s, owner);
            }
        }
    }
}
