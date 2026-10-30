// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
//! The definitions macros build with `quotes.reflect`: new value symbols, their `ValDef`s and
//! copies of definitions with other right-hand sides.

use super::Reflect;
use crate::tpd::{TNodeId, Tree as T};
use tasty::tags::{
    FINAL, GIVEN, IMPLICIT, LAZY, LOCAL, MUTABLE, OVERRIDE, PRIVATE, PROTECTED, STATIC, SYNTHETIC,
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
                match *self.u.types().get(info) {
                    Type::ByName(result) => (flags & !(1 << METHOD), result),
                    _ => (flags & !(1 << METHOD), info),
                }
            }
        };
        if flags & !allowed.iter().fold(0, |b, &t| b | 1u64 << t) != 0 {
            return Err("Symbol.newVal or Symbol.newMethod with flags other than modifiers of definitions");
        }
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
        Ok(s)
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

    /// New parameter definitions of method `s` by clause, from its type (the parameter symbols
    /// of dotty's `DefDef(sym, rhsFn)`).
    pub fn method_params(&mut self, s: SymbolId) -> Result<Vec<Vec<TNodeId>>, &'static str> {
        let (span, mut t) = (self.span, self.u.info(s));
        let mut clauses = Vec::new();
        loop {
            match self.u.types().get(t).clone() {
                Type::Method { params, result, .. } => {
                    let mut clause = Vec::new();
                    for (name, info) in params {
                        let p = self.new_param(s, &name, info);
                        let tpt = self.type_tree(info);
                        clause.push(self.build()?.val_def(p, tpt, None, span));
                    }
                    clauses.push(clause);
                    t = result;
                }
                Type::Poly { .. } => return Err("DefDef.apply of a polymorphic method"),
                _ => return Ok(clauses),
            }
        }
    }

    fn new_param(&mut self, owner: SymbolId, name: &str, info: TypeId) -> SymbolId {
        let p = self.u.define(Symbol {
            name: name.to_owned(),
            kind: Kind::Param,
            owner: None,
            flags: Flags::default(),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        self.u.set_owner(p, owner);
        self.u.set_info(p, info);
        p
    }

    /// `DefDef(s, rhsFn)` of method `s` with parameter definitions `params` and the right-hand
    /// side `rhsFn` made.
    pub fn def_def(
        &mut self,
        s: SymbolId,
        params: &[Vec<TNodeId>],
        rhs: Option<TNodeId>,
    ) -> Result<TNodeId, &'static str> {
        if self.u.symbol(s).kind != Kind::Def {
            return Err("DefDef.apply of a symbol other than a method");
        }
        let mut result = self.u.info(s);
        while let Type::Method { result: r, .. } = *self.u.types().get(result) {
            result = r;
        }
        let (span, tpt) = (self.span, self.type_tree(result));
        let clauses: Vec<&[TNodeId]> = params.iter().map(Vec::as_slice).collect();
        Ok(self.build()?.def_def(s, &clauses, tpt, rhs, span))
    }
}
