// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! The classes macros make with `quotes.reflect`: `Symbol.newClass`, `ClassDef.apply`, and the
//! `New` and `TypeIdent` trees that instantiate them.

use super::Reflect;
use crate::defn::type_ref;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::TName;
use tasty::{Flags, Kind, Symbol, SymbolId, Type, TypeId};

impl Reflect<'_> {
    /// `Symbol.newClass(owner, name, parents, _, None)`: a class of `parents` with a constructor
    /// without parameters, its declarations entered by `enter_decls`.
    pub fn new_class(
        &mut self,
        owner: SymbolId,
        name: &str,
        parents: Vec<TypeId>,
    ) -> Result<SymbolId, &'static str> {
        if parents.iter().any(|&p| self.u.class_symbol(p).is_none()) {
            return Err("Symbol.newClass with a parent other than a class type");
        }
        let class = self.define(name, Kind::Class, owner);
        self.u.set_parents(class, parents);
        let ctor = self.define("<init>", Kind::Def, class);
        self.u.enter(ctor);
        let binder = self.u.fresh_binder();
        let result = type_ref(self.u, class);
        let info = self.u.intern_type(Type::Method {
            binder,
            params: Vec::new(),
            result,
            implicit: false,
            contextual: false,
        });
        self.u.set_info(ctor, info);
        let dummy = self.define(&format!("<local {name}>"), Kind::Val, class);
        self.u.enter(dummy);
        Ok(class)
    }

    /// Enters `decls`, symbols owned by `class`, into its declarations.
    pub fn enter_decls(&mut self, class: SymbolId, decls: &[SymbolId]) -> Result<(), &'static str> {
        for &d in decls {
            if self.u.symbol(d).owner != Some(class) {
                return Err("Symbol.newClass with a declaration of another owner");
            }
            if !self.u.symbol(class).decls.contains(&d) {
                self.u.enter(d);
            }
        }
        Ok(())
    }

    /// `ClassDef(cls, parents, body)` of a class `Symbol.newClass` made, its class parents called
    /// as constructors without arguments.
    pub fn class_def(
        &mut self,
        cls: SymbolId,
        parents: &[TNodeId],
        body: &[TNodeId],
    ) -> Result<TNodeId, &'static str> {
        let span = self.span;
        let ctor = self.u.decl(cls, "<init>").ok().and_then(|c| c.first().copied());
        let ctor = ctor.ok_or("ClassDef.apply of a class without a constructor")?;
        let dummy = format!("<local {}>", self.u.symbol(cls).name);
        let dummy = crate::defn::entered(self.u, cls, &dummy, Kind::Val);
        let unit = self.u.class_ref("scala.Unit");
        let tpt = self.type_tree(unit);
        let constr = self.build()?.def_def(ctor, &[&[]], tpt, None, span);
        let mut called = Vec::new();
        for &p in parents {
            called.push(self.parent_call(p)?);
        }
        let (parents, body) = (self.tpd.list(&called), self.tpd.list(body));
        let template = self.tpd.node(T::Template { constr, parents, self_: None, body }, span, None, dummy);
        let name = TName { name: self.names.intern(&self.u.symbol(cls).name), is_type: true };
        let tpe = type_ref(self.u, cls);
        Ok(self.tpd.node(T::TypeDef { name, rhs: template }, span, Some(tpe), Some(cls)))
    }

    /// Parent tree `p`: a constructor call or a trait's type tree as it is, a class's type tree
    /// as a call of its constructor without arguments.
    fn parent_call(&mut self, p: TNodeId) -> Result<TNodeId, &'static str> {
        let span = self.span;
        let tpe = self.tpd.tpe(p);
        let c = self.u.class_symbol(tpe).ok_or("ClassDef.apply with a parent other than a class type")?;
        let type_tree =
            matches!(self.tpd.tree(p), T::TypeTree { .. } | T::Ident { .. } | T::AppliedTypeTree { .. })
                || matches!(self.tpd.tree(p), T::Select { name, .. } if name.is_type);
        if self.u.is_trait(c) || !type_tree {
            return Ok(p);
        }
        let ctors = self.u.decl(c, "<init>").unwrap_or_default();
        let no_args = |r: &mut Self, k: SymbolId| {
            let info = r.u.info(k);
            matches!(r.u.types().get(info), Type::Method { params, result, .. }
                if params.is_empty() && !matches!(r.u.types().get(*result), Type::Method { .. }))
        };
        let ctor = ctors.into_iter().find(|&k| no_args(self, k));
        let ctor =
            ctor.ok_or("ClassDef.apply with a parent class without a constructor without arguments")?;
        let new = self.tpd.node(T::New { tpt: p }, span, Some(tpe), None);
        let mut b = self.build()?;
        let select = b.select(new, ctor, span);
        Ok(b.apply(select, &[], tpe, span))
    }

    /// `TypeIdent(sym)`.
    pub fn type_ident(&mut self, s: SymbolId) -> Result<TNodeId, &'static str> {
        if !self.u.is_type_symbol(s) {
            return Err("TypeIdent of a term");
        }
        let name = TName { name: self.names.intern(&self.u.symbol(s).name), is_type: true };
        let tpe = type_ref(self.u, s);
        Ok(self.tpd.node(T::Ident { name }, self.span, Some(tpe), Some(s)))
    }

    /// `New(tpt)`.
    pub fn new_(&mut self, tpt: TNodeId) -> TNodeId {
        let tpe = self.tpd.tpe(tpt);
        self.tpd.node(T::New { tpt }, self.span, Some(tpe), None)
    }

    fn define(&mut self, name: &str, kind: Kind, owner: SymbolId) -> SymbolId {
        let s = self.u.define(Symbol {
            name: name.to_owned(),
            kind,
            owner: None,
            flags: Flags::default(),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        self.u.set_owner(s, owner);
        s
    }
}
