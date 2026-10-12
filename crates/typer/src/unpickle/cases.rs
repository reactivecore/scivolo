//! Matches, `try` and `throw`, and the patterns of their cases (dotty's `MATCH`, `TRY`, `THROW`,
//! `CASEDEF`, `BIND`, `ALTERNATIVE`, `UNAPPLY`), typed as `TypeAssigner` types them.

use super::{Refusal, Unpickler};
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::TName;
use tasty::tags::*;
use tasty::{Flags, Kind, NodeId, Symbol, SymbolId, Type, TypeId, View, view};

impl Unpickler<'_> {
    /// `MATCH` of a selector, neither inline nor a `summonFrom`; typed as its cases' lub.
    pub fn match_(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let View::Match { prefix: None, sel: Some(sel), cases } = view(self.ast(), id) else {
            return Err("inline match in a library inline body");
        };
        let cases = cases.to_vec();
        let selector = self.term(sel)?;
        let cases = cases.into_iter().map(|c| self.case_def(c)).collect::<Result<Vec<_>, _>>()?;
        let tpe = self.lub_of(&cases);
        let cases = self.tpd.list(&cases);
        Ok(self.bare(T::Match { selector, cases }, Some(tpe), None))
    }

    /// `TRY`: typed as the lub of its expression and cases.
    pub fn try_(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let k = self.kids(id);
        let expr = self.term(k[0])?;
        let mut cases = Vec::new();
        let mut finalizer = None;
        for &c in &k[1..] {
            match self.tag(c) {
                CASEDEF => cases.push(self.case_def(c)?),
                _ => finalizer = Some(self.term(c)?),
            }
        }
        let tpe = self.lub_of(&[&[expr][..], &cases].concat());
        let cases = self.tpd.list(&cases);
        Ok(self.bare(T::Try { expr, cases, finalizer }, Some(tpe), None))
    }

    /// `THROW`: a call of dotty's `<special-ops>.throw`, its function at the call's span.
    pub fn throw(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let expr = self.term(self.kids(id)[0])?;
        let throw = self.throw_method()?;
        let fun_tpe = self.u.intern_type(Type::Sym(None, throw));
        let name = TName { name: self.names.intern("throw"), is_type: false };
        let fun = self.bare(T::Ident { name }, Some(fun_tpe), Some(throw));
        let nothing = self.class_type("scala.Nothing")?;
        let args = self.tpd.list(&[expr]);
        let t = self.bare(T::Apply { fun, args }, Some(nothing), None);
        let t = self.positioned(id, t);
        self.tpd.get_mut(fun).span = self.tpd.span(t).to_synthetic();
        Ok(t)
    }

    fn case_def(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let View::CaseDef { pat, rhs, guard } = view(self.ast(), id) else {
            return Err("case of another kind in a library inline body");
        };
        let pat = self.term(pat)?;
        let guard = guard.map(|g| self.term(g)).transpose()?;
        let body = self.term(rhs)?;
        let t = self.bare(T::CaseDef { pat, guard, body }, None, None);
        Ok(self.positioned(id, t))
    }

    /// `BIND` of a term: its symbol defined with the type the tree states.
    pub fn bind(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let k = self.kids(id);
        if matches!(self.tag(self.ast().deref(k[0])), TYPEBOUNDS) {
            return Err("type binding in a library inline body");
        }
        let sym = match self.u.defined_at(self.file, id) {
            Some(s) => s,
            None => self.define_bind(id, k[0]),
        };
        let info = self.u.info(sym);
        self.check(info)?;
        let body = self.term(k[1])?;
        let name = self.tname(id, false);
        let tpe = self.u.intern_type(Type::Sym(None, sym));
        Ok(self.bare(T::Bind { name, body }, Some(tpe), Some(sym)))
    }

    fn define_bind(&mut self, id: NodeId, tpe: NodeId) -> SymbolId {
        let s = self.u.define(Symbol {
            name: self.name(id),
            kind: Kind::Val,
            owner: Some(self.owner),
            flags: Flags::default().with(CASE),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: Some((self.file, id)),
        });
        self.u.unenter(s);
        let info = self.ty(tpe);
        self.u.set_info(s, info);
        s
    }

    /// `ALTERNATIVE`: typed as the lub of its alternatives.
    pub fn alternative(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let trees = self.kids(id).into_iter().map(|t| self.term(t)).collect::<Result<Vec<_>, _>>()?;
        let tpe = self.lub_of(&trees);
        let trees = self.tpd.list(&trees);
        Ok(self.bare(T::Alternative { trees }, Some(tpe), None))
    }

    /// `UNAPPLY`: the extractor, its implicit arguments and patterns, of the type the tree states.
    pub fn unapply(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let k = self.kids(id);
        let fun = self.term(k[0])?;
        let n = k[1..].iter().take_while(|&&a| self.tag(a) == IMPLICITarg).count();
        let implicits =
            k[1..1 + n].iter().map(|&a| self.term(self.kids(a)[0])).collect::<Result<Vec<_>, _>>()?;
        let tpe = self.ty(k[1 + n]);
        let patterns = k[2 + n..].iter().map(|&p| self.term(p)).collect::<Result<Vec<_>, _>>()?;
        let (implicits, patterns) = (self.tpd.list(&implicits), self.tpd.list(&patterns));
        Ok(self.bare(T::UnApply { fun, implicits, patterns }, Some(tpe), None))
    }

    /// The lub of the types of `trees`, of a case its body's.
    fn lub_of(&mut self, trees: &[TNodeId]) -> TypeId {
        crate::tpd::lub_of(self.tpd, self.u, trees)
    }

    /// dotty's `<special-ops>.throw(x: Throwable): Nothing`, defined if no one has.
    fn throw_method(&mut self) -> Result<SymbolId, Refusal> {
        let ops = self.u.enter_package("<special-ops>");
        if let Some(t) = crate::defn::entered(self.u, ops, "throw", Kind::Def) {
            return Ok(t);
        }
        let throwable = self.class_type("java.lang.Throwable")?;
        let nothing = self.class_type("scala.Nothing")?;
        let binder = self.u.fresh_binder();
        let params = vec![("x".into(), throwable)];
        let info = self.u.intern_type(Type::Method {
            binder,
            params,
            result: nothing,
            implicit: false,
            contextual: false,
        });
        let throw = self.special(ops, "throw", Kind::Def, Flags::default());
        self.u.set_info(throw, info);
        Ok(throw)
    }

    /// The symbol `name` of kind `kind` in `owner`, defined with `flags` if no one has.
    pub fn special(&mut self, owner: SymbolId, name: &str, kind: Kind, flags: Flags) -> SymbolId {
        if let Some(s) = crate::defn::entered(self.u, owner, name, kind) {
            return s;
        }
        self.u.define(Symbol {
            name: name.into(),
            kind,
            owner: Some(owner),
            flags,
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        })
    }
}
