// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreeUnpickler.scala
//! Definitions (dotty's `readIndexedDef`): the inline method with its parameters, and the local
//! values and methods of its body, their symbols defined when their block is read.

use super::{Refusal, Unpickler};
use crate::tpd::{Build, TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::tags::*;
use tasty::{Flags, Kind, NodeId, Symbol, SymbolId, View, view};

impl Unpickler<'_> {
    /// The definition of the inline method at `id`.
    pub fn inline_def(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        if !matches!(view(self.ast(), id), View::DefDef { .. }) {
            return Err("inline method of another kind");
        }
        let def = self.def_def(id, self.meth)?;
        match self.tpd.tree(def) {
            T::DefDef { rhs: Some(_), .. } => Ok(def),
            _ => Err("inline method without a body"),
        }
    }

    /// The definition of the value or method at `id`.
    pub fn member_def(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        match view(self.ast(), id) {
            View::DefDef { .. } => self.def_def(id, self.meth),
            View::ValDef { .. } => self.local_def(id),
            _ => Err("a definition of another kind than a value or method"),
        }
    }

    /// The definition at `id` of method `sym`, which owns the locals of its body.
    fn def_def(&mut self, id: NodeId, sym: SymbolId) -> Result<TNodeId, Refusal> {
        let View::DefDef { parts, .. } = view(self.ast(), id) else {
            return Err("definition of another kind in a library inline body");
        };
        let (params, body) = (parts.params.to_vec(), parts.body.to_vec());
        let mut paramss = Vec::new();
        for clause in tasty::param_tree_clauses(self.ast(), &params) {
            let params: Vec<NodeId> =
                clause.into_iter().filter(|&p| matches!(self.tag(p), PARAM | TYPEPARAM)).collect();
            paramss.push(params.into_iter().map(|p| self.param(p)).collect::<Result<Vec<_>, _>>()?);
        }
        let tpt = self.tpt(*body.first().ok_or("method without a result type in a library inline body")?)?;
        let outer = std::mem::replace(&mut self.owner, sym);
        let rhs = body.get(1).map(|&r| self.term(r)).transpose();
        self.owner = outer;
        let clauses: Vec<&[TNodeId]> = paramss.iter().map(Vec::as_slice).collect();
        let unit = self.class_symbol("scala.Unit")?;
        let def =
            Build::new(self.tpd, self.u, self.names, unit).def_def(sym, &clauses, tpt, rhs?, Span::NONE);
        Ok(self.positioned(id, def))
    }

    /// The parameter defined at `id`, its type tree read for a local method, else the type it is
    /// entered with.
    pub fn param(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let sym =
            self.u.defined_at(self.file, id).ok_or("parameter without a symbol in a library inline body")?;
        let info = self.u.info(sym);
        self.check(info)?;
        let tpt = match self.u.symbol(sym).owner == Some(self.meth) {
            true => self.bare(T::TypeTree { inferred: false }, Some(info), None),
            false => self.tpt(self.kids(id)[0])?,
        };
        let tpt = self.positioned(self.kids(id)[0], tpt);
        let is_type = self.tag(id) == TYPEPARAM;
        let name = self.tname(id, is_type);
        let tree = match is_type {
            true => T::TypeDef { name, rhs: tpt },
            false => T::ValDef { name, tpt, rhs: None },
        };
        let t = self.bare(tree, None, Some(sym));
        Ok(self.positioned(id, t))
    }

    /// The local value or method defined at `id`, its symbol defined before.
    pub fn local_def(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let sym =
            self.u.defined_at(self.file, id).ok_or("local without a symbol in a library inline body")?;
        let View::ValDef { tpt, rhs, .. } = view(self.ast(), id) else { return self.def_def(id, sym) };
        let tpt = self.tpt(tpt)?;
        let outer = std::mem::replace(&mut self.owner, sym);
        let rhs = rhs.map(|r| self.term(r)).transpose();
        self.owner = outer;
        let unit = self.class_symbol("scala.Unit")?;
        let def = Build::new(self.tpd, self.u, self.names, unit).val_def(sym, tpt, rhs?, Span::NONE);
        Ok(self.positioned(id, def))
    }

    /// Defines the symbol of the local value or method at `id`, owned by the current owner but
    /// not among its declarations, and those of its parameters; the one defined before if it was
    /// read already.
    pub fn define_local(&mut self, id: NodeId) -> Result<SymbolId, Refusal> {
        self.define_definition(id, self.owner, false)
    }

    /// Defines the symbol of the definition at `id`, owned by `owner` and among its declarations
    /// if `member`: a class with its members, a parameter of a class as its value.
    pub fn define_definition(
        &mut self,
        id: NodeId,
        owner: SymbolId,
        member: bool,
    ) -> Result<SymbolId, Refusal> {
        if let Some(s) = self.u.defined_at(self.file, id) {
            return Ok(s);
        }
        let in_class = self.u.symbol(owner).kind == Kind::Class;
        let (kind, mods, params) = match view(self.ast(), id) {
            View::TypeDef { is_class: true, .. } => return self.define_class(id, owner, member),
            View::TypeDef { mods, .. } => (Kind::Type, mods.to_vec(), Vec::new()),
            View::DefDef { parts, .. } => (Kind::Def, parts.mods.to_vec(), parts.params.to_vec()),
            View::ValDef { mods, .. } => (Kind::Val, mods.to_vec(), Vec::new()),
            View::Param { mods, .. } if self.tag(id) == TYPEPARAM => {
                (Kind::TypeParam, mods.to_vec(), Vec::new())
            }
            View::Param { mods, .. } if in_class => (Kind::Val, mods.to_vec(), Vec::new()),
            View::Param { mods, .. } => (Kind::Param, mods.to_vec(), Vec::new()),
            _ => return Err("local definition of another kind in a library inline body"),
        };
        let flags = mods.iter().fold(Flags::default(), |f, &m| f.with(self.tag(m)));
        let flags = match in_class && self.tag(id) == PARAM {
            true => flags.with(PARAMsetter),
            false => flags,
        };
        let s = self.u.define(Symbol {
            name: self.name(id),
            kind,
            owner: Some(owner),
            flags,
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: Some((self.file, id)),
        });
        if !member {
            self.u.unenter(s);
        }
        self.annotate_quoted(s, &mods);
        for p in params {
            if matches!(self.tag(p), PARAM | TYPEPARAM) {
                self.define_definition(p, s, true)?;
            }
        }
        let info = self.u.info(s);
        self.check(info)?;
        let normalized = crate::typer::seen_from::normalized(self.u, info);
        if normalized != info {
            self.u.set_info(s, normalized);
        }
        Ok(s)
    }
}
