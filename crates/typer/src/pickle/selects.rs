// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//   compiler/src/dotty/tools/dotc/transform/PostTyper.scala
//! Identifiers and selections.

use super::names::sym_name;
use super::{Refusal, TreePickler};
use crate::defn::type_ref;
use crate::signatures::result_sig;
use crate::tpd::TNodeId;
use names::{name_kinds, nme};
use syntax::ast::TName;
use tasty::tags::*;
use tasty::write::Name;
use tasty::{SymbolId, Type, TypeId};

impl TreePickler<'_> {
    /// The name of tree name `name`: a default getter's, else `sym`'s name if it is the symbol's.
    pub fn tree_name(&self, name: TName, sym: Option<SymbolId>) -> Name {
        let text = self.names.text(name.name);
        match sym {
            Some(s) if text.contains(name_kinds::DEFAULT_GETTER) => self.ref_name(s),
            Some(s) if self.u.symbol(s).name == text => sym_name(self.u, s),
            _ => Name::utf8(text),
        }
    }

    /// Identifier `id`: a reference to a self as its class's `this`, a path as its type, else its
    /// name and type.
    pub fn ident(
        &mut self,
        id: TNodeId,
        name: TName,
        tpe: TypeId,
        sym: Option<SymbolId>,
    ) -> Result<(), Refusal> {
        if let Some(class) = self.u.this_of(tpe) {
            return self.class_this(id, None, class);
        }
        if self.is_term_ref(tpe) && self.names.text(name.name) != nme::WILDCARD {
            return self.ty(tpe, false);
        }
        self.byte(if name.is_type { IDENTtpt } else { IDENT });
        self.name(&self.tree_name(name, sym));
        self.ty(tpe, false)
    }

    /// Selection `id`, of a method through its owner unless the qualifier is a hole.
    pub fn select(
        &mut self,
        id: TNodeId,
        qual: TNodeId,
        name: TName,
        sym: Option<SymbolId>,
    ) -> Result<(), Refusal> {
        if let Some(&acc) = self.super_calls.get(&id) {
            return self.super_accessor_select(id, acc);
        }
        let method = sym.filter(|&s| !name.is_type && result_sig(self.u, s).is_some());
        let Some(s) = method.filter(|_| !self.holes.contains_key(&qual)) else {
            self.byte(if name.is_type { SELECTtpt } else { SELECT });
            match method {
                Some(s) => self.signed_name(s, true)?,
                None => self.name(&self.tree_name(name, sym)),
            }
            return match name.is_type {
                true => self.tree(qual),
                false => self.qualifier(id, qual),
            };
        };
        let owner = self.u.symbol(s).owner.ok_or("a method without owner")?;
        let owner = type_ref(self.u, owner);
        self.with_length(SELECTin, &mut |p| {
            p.signed_name(s, true)?;
            p.qualifier(id, qual)?;
            p.ty(owner, false)
        })
    }

    /// Qualifier `qual` of term selection `id`, a package selected the package object `id` is a
    /// member of (dotty's `PostTyper.transformSelect`).
    fn qualifier(&mut self, id: TNodeId, qual: TNodeId) -> Result<(), Refusal> {
        let types = self.u.types();
        let object = match (
            self.tpd.get(qual).tpe.map(|t| *types.get(t)),
            self.tpd.get(id).tpe.map(|t| *types.get(t)),
        ) {
            (Some(Type::Package(_)), Some(Type::Sym(Some(pre), _))) => match *types.get(pre) {
                Type::Sym(None, m) if self.u.is_module(m) => Some(m),
                _ => None,
            },
            _ => None,
        };
        if let Some(m) = object {
            self.byte(SELECT);
            self.name(&sym_name(self.u, m));
        }
        self.tree(qual)
    }
}
