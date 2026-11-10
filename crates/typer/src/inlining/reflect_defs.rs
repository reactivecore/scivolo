// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The definitions macros build with `quotes.reflect`: new value symbols, their `ValDef`s and
//! copies of definitions with other right-hand sides.

use super::Reflect;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::TName;
use tasty::tags::{
    FINAL, GIVEN, IMPLICIT, INFIX, LAZY, LOCAL, MUTABLE, OVERRIDE, PRIVATE, PROTECTED, STATIC, SYNTHETIC,
};
use tasty::{Flags, Kind, Symbol, SymbolId, Type, TypeId};

/// The bit of `Flags.Method` among the flags of `quotes.reflect`.
const METHOD: u32 = 2;

impl Reflect<'_> {
    /// `Symbol.newVal` and `Symbol.newMethod(owner, name, tpe, flags, _)` by `kind`, not entered
    /// into its owner's declarations; a method of by-name type `=> T` has result type `T`, a
    /// static one is a member of a top-level class.
    pub fn new_term(
        &mut self,
        (owner, name, kind): (SymbolId, &str, Kind),
        info: TypeId,
        flags: u64,
    ) -> Result<SymbolId, &'static str> {
        let mut allowed = vec![PRIVATE, PROTECTED, FINAL, IMPLICIT, OVERRIDE, LOCAL, SYNTHETIC, GIVEN];
        let (flags, info) = match kind {
            Kind::Val => {
                allowed.extend([LAZY, MUTABLE]);
                (flags, info)
            }
            _ => {
                let in_package =
                    self.u.symbol(owner).owner.is_some_and(|o| self.u.symbol(o).kind == Kind::Package);
                if self.u.is_class(owner) && in_package {
                    allowed.push(STATIC);
                }
                (flags & !(1 << METHOD), self.u.by_name(info).unwrap_or(info))
            }
        };
        if flags & !allowed.iter().fold(0, |b, &t| b | 1u64 << t) != 0 {
            return Err("Symbol.newVal or Symbol.newMethod with flags other than modifiers of definitions");
        }
        Ok(self.defined((owner, name, kind), flags, info))
    }

    /// `Symbol.newTypeAlias(owner, name, flags, tpe, _)`, not entered into its owner's
    /// declarations.
    pub fn new_type_alias(
        &mut self,
        (owner, name): (SymbolId, &str),
        alias: TypeId,
        flags: u64,
    ) -> Result<SymbolId, &'static str> {
        let allowed = [PRIVATE, PROTECTED, OVERRIDE, FINAL, INFIX, LOCAL];
        if flags & !allowed.iter().fold(0, |b, &t| b | 1u64 << t) != 0 {
            return Err("Symbol.newTypeAlias with flags other than modifiers of type aliases");
        }
        if matches!(self.u.ty(alias), Type::Bounds(..) | Type::Alias(_)) {
            return Err("Symbol.newTypeAlias of type bounds");
        }
        let info = self.u.intern_type(Type::Alias(alias));
        Ok(self.defined((owner, name, Kind::Type), flags, info))
    }

    fn defined(&mut self, (owner, name, kind): (SymbolId, &str, Kind), flags: u64, info: TypeId) -> SymbolId {
        let s = self.u.define(Symbol {
            name: name.to_owned(),
            kind,
            owner: None,
            flags: Flags(flags),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        self.u.set_owner(s, owner);
        self.u.set_info(s, info);
        s
    }

    /// `TypeDef(s)` of type alias `s`, its right-hand side a type tree of the alias.
    pub fn type_def(&mut self, s: SymbolId) -> Result<TNodeId, &'static str> {
        let info = self.u.info(s);
        let (Kind::Type, Some(alias)) = (self.u.symbol(s).kind, self.u.type_alias(info)) else {
            return Err("TypeDef.apply of a symbol other than a type alias");
        };
        let rhs = self.type_tree(alias);
        let name = TName { name: self.names.intern(&self.u.symbol(s).name.clone()), is_type: true };
        Ok(self.tpd.node(T::TypeDef { name, rhs }, self.span, None, Some(s)))
    }

    /// `ValDef(s, rhs)`.
    pub fn val_def(&mut self, s: SymbolId, rhs: Option<TNodeId>) -> Result<TNodeId, &'static str> {
        if self.u.symbol(s).kind != Kind::Val {
            return Err("ValDef.apply of a symbol other than a value");
        }
        let (span, info) = (self.span, self.u.info(s));
        let tpt = self.type_tree(info);
        Ok(self.build()?.val_def(s, tpt, rhs, span))
    }

    /// `ValDef.copy(original)(name, tpt, rhs)` and `DefDef.copy(original)(name, paramss, tpt, rhs)`
    /// with the parameters of `original`: its symbol and name kept.
    pub fn copy_def(
        &mut self,
        original: TNodeId,
        (name, params): (&str, &[TNodeId]),
        tpt: TNodeId,
        rhs: Option<TNodeId>,
    ) -> Result<TNodeId, &'static str> {
        let s = self.tpd.sym(original).ok_or("a copy of a definition without a symbol")?;
        if self.u.symbol(s).name != name {
            return Err("a copy of a definition under another name");
        }
        let tree = match *self.tpd.tree(original) {
            T::ValDef { name, tpt: t, rhs: r } if params.is_empty() => {
                if (t, r) == (tpt, rhs) {
                    return Ok(original);
                }
                T::ValDef { name, tpt, rhs }
            }
            T::DefDef { name, paramss, tpt: t, rhs: r } => {
                let old = self.tpd.clause_items(paramss).iter().flat_map(|&l| self.tpd.items(l).to_vec());
                if !old.eq(params.iter().copied()) {
                    return Err("DefDef.copy with other parameters");
                }
                if (t, r) == (tpt, rhs) {
                    return Ok(original);
                }
                T::DefDef { name, paramss, tpt, rhs }
            }
            _ => return Err("ValDef.copy or DefDef.copy of another tree"),
        };
        Ok(self.tpd.node(tree, self.tpd.span(original), None, Some(s)))
    }

    /// `ClassDef.copy(original)(name, constr, parents, self, body)` with the name, constructor,
    /// self and parents of `original`, a parent also as a type tree of its class: its class
    /// and parent trees kept.
    pub fn copy_class(
        &mut self,
        original: TNodeId,
        (name, constr, self_): (&str, TNodeId, Option<TNodeId>),
        parents: &[TNodeId],
        body: &[TNodeId],
    ) -> Result<TNodeId, &'static str> {
        let refused = "ClassDef.copy with another name, constructor, parents or self";
        let T::TypeDef { name: n, rhs } = *self.tpd.tree(original) else {
            return Err("ClassDef.copy of another tree");
        };
        let T::Template { constr: c, parents: ps, self_: s, .. } = *self.tpd.tree(rhs) else {
            return Err("ClassDef.copy of another tree");
        };
        let old = self.tpd.items(ps).to_vec();
        let kept = |r: &mut Self, (o, p): (TNodeId, TNodeId)| {
            let type_tree =
                matches!(r.tpd.tree(p), T::TypeTree { .. } | T::Ident { .. } | T::AppliedTypeTree { .. });
            o == p || type_tree && r.u.class_symbol(r.tpd.tpe(o)) == r.u.class_symbol(r.tpd.tpe(p))
        };
        let same_parents = old.len() == parents.len()
            && old.into_iter().zip(parents.iter().copied()).all(|op| kept(self, op));
        if self.names.text(n.name) != name || c != constr || s != self_ || !same_parents {
            return Err(refused);
        }
        let body = self.tpd.list(body);
        let template = T::Template { constr, parents: ps, self_, body };
        let (span, sym) = (self.tpd.span(rhs), self.tpd.sym(rhs));
        let rhs = self.tpd.node(template, span, None, sym);
        let (span, tpe, sym) = (self.tpd.span(original), self.tpd.get(original).tpe, self.tpd.sym(original));
        Ok(self.tpd.node(T::TypeDef { name: n, rhs }, span, tpe, sym))
    }
}
