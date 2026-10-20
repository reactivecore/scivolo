// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreeUnpickler.scala
//! Identifiers and selections, members resolved by name and signature (dotty's `IDENT`,
//! `completeSelect`, `SELECTin`, `QUALTHIS`).

use super::{Refusal, Unpickler};
use crate::defn::{ref_type, type_ref};
use crate::signatures::params_sig;
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::stability::is_stable_member;
use tasty::tags::*;
use tasty::{Kind, NodeId, SymbolId, Type, TypeId};

impl Unpickler<'_> {
    /// `IDENT`: an identifier of the type the tree states.
    pub fn ident(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let tpe = self.ty(self.kids(id)[0]);
        let tpe = self.resolved(tpe);
        let name = self.tname(id, false);
        if self.name(id) == "_" {
            let sym = match self.u.types().get(tpe).clone() {
                Type::TypeRef(p, n) => self.u.resolve(p, &n, true),
                Type::Sym(_, s) => Some(s),
                _ => None,
            };
            return Ok(self.bare(T::Ident { name }, Some(tpe), sym));
        }
        let sym = self.symbol_of(tpe).ok_or("identifier of an unknown symbol in a library inline body")?;
        Ok(self.bare(T::Ident { name }, Some(tpe), Some(sym)))
    }

    /// `SELECT`, `SELECTtpt`: the member of the qualifier's type of the name and signature.
    pub fn select(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let is_type = self.tag(id) == SELECTtpt;
        let qual = self.term(self.kids(id)[0])?;
        let tpe = self.tpd.tpe(qual);
        let prefix = self.widen_if_unstable(tpe);
        let name = self.name(id);
        let found = match *self.u.types().get(prefix) {
            Type::Package(p) => self.u.decl(p, &name).unwrap_or_default(),
            _ => self.u.member_of_type(prefix, &name),
        };
        let sym = self.at_signature(found, id, is_type)?;
        Ok(self.selection(id, qual, prefix, sym, is_type))
    }

    /// `SELECTin`: the member of the class the tree states, else of its base classes.
    pub fn select_in(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let k = self.kids(id);
        let qual = self.term(k[0])?;
        let owner = self.ty(k[1]);
        let owner = match self.u.types().get(owner).clone() {
            Type::TypeRef(p, n) => self.u.resolve(p, &n, true),
            _ => self.u.class_symbol(owner),
        };
        let owner = owner.ok_or("selection in an unknown class in a library inline body")?;
        let tpe = self.tpd.tpe(qual);
        let prefix = self.widen_if_unstable(tpe);
        let name = self.name(id);
        let decls = self.u.decl(owner, &name).unwrap_or_default();
        let sym = match self.at_signature(decls, id, false) {
            Ok(s) => s,
            Err(_) => {
                let inherited = self.u.member_where(owner, &name, |u, m| !u.symbol(m).flags.has(PRIVATE));
                self.at_signature(inherited, id, false)?
            }
        };
        Ok(self.selection(id, qual, prefix, sym, false))
    }

    fn selection(
        &mut self,
        id: NodeId,
        qualifier: TNodeId,
        prefix: TypeId,
        sym: SymbolId,
        is_type: bool,
    ) -> TNodeId {
        let tpe = ref_type(self.u, Some(prefix), sym);
        let name = self.tname(id, is_type);
        self.bare(T::Select { qualifier, name }, Some(tpe), Some(sym))
    }

    /// The one symbol among `found` of the kind and signature of the name at `id` (dotty's
    /// `atSignature`).
    pub fn at_signature(
        &mut self,
        found: Vec<SymbolId>,
        id: NodeId,
        is_type: bool,
    ) -> Result<SymbolId, Refusal> {
        let sig = self.signature(id);
        let found: Vec<SymbolId> =
            found.into_iter().filter(|&s| self.u.is_type_symbol(s) == is_type).collect();
        let matching: Vec<SymbolId> = match sig {
            Some(sig) => found
                .into_iter()
                .filter(|&s| self.u.symbol(s).kind == Kind::Def && params_sig(self.u, s) == sig)
                .collect(),
            None if found.len() == 1 => found,
            None => found.into_iter().filter(|&s| self.u.symbol(s).kind != Kind::Def).collect(),
        };
        match matching[..] {
            [s] => Ok(s),
            [] => Err("selection of an unknown member in a library inline body"),
            _ => Err("selection of an overloaded member in a library inline body"),
        }
    }

    /// `C.this`, its qualifier the untyped name of the class.
    pub fn qual_this(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let qual_id = self.kids(id)[0];
        let class = self.ty(qual_id);
        let cls = self.u.class_symbol(class).ok_or("this of an unknown class in a library inline body")?;
        let tpe = type_ref(self.u, cls);
        let name = self.tname(qual_id, true);
        let qual = self.bare(T::Ident { name }, None, None);
        let qual = self.positioned(qual_id, qual);
        let tpe = self.u.intern_type(Type::This(tpe));
        Ok(self.bare(T::This { qual }, Some(tpe), Some(cls)))
    }

    /// `t` widened while it is a by-name type or a reference to a term that is not stable
    /// (dotty's `widenIfUnstable`), an inline parameter counting as stable, unlike in dotty's
    /// `isStableMember`, as dotty's prefixes of selections in inline bodies show.
    fn widen_if_unstable(&mut self, t: TypeId) -> TypeId {
        match self.u.types().get(t).clone() {
            Type::ByName(r) => self.widen_if_unstable(r),
            Type::Sym(p, s) if self.u.symbol(s).kind.is_term() && !self.is_stable(s) => {
                let info = match p.filter(|&p| !matches!(self.u.types().get(p), Type::Package(_))) {
                    Some(p) => self.u.member_info(p, s),
                    None => self.u.info(s),
                };
                self.widen_if_unstable(info)
            }
            _ => t,
        }
    }

    fn is_stable(&mut self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        let inline_param = sym.kind == Kind::Param && sym.flags.has(INLINE);
        if inline_param {
            let info = self.u.info(s);
            return !matches!(self.u.types().get(info), Type::ByName(_));
        }
        is_stable_member(self.u, s)
    }
}
