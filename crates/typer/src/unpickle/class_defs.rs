// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreeUnpickler.scala
//! Local classes and type aliases of library inline bodies: their symbols with the members of
//! the template (dotty's `createMemberSymbol`, `indexStats`) and their definitions
//! (`readTemplate`).

use super::{Refusal, Unpickler};
use crate::tpd::{TNodeId, Tree as T};
use tasty::tags::*;
use tasty::{Flags, Kind, NodeId, Symbol, SymbolId, Type, TypeId, View, view};

impl Unpickler<'_> {
    /// Defines the class at `id`, owned by `owner` and among its declarations if `member`, with
    /// the members of its template and its local dummy; the one defined before if read already.
    pub fn define_class(&mut self, id: NodeId, owner: SymbolId, member: bool) -> Result<SymbolId, Refusal> {
        if let Some(s) = self.u.defined_at(self.file, id) {
            return Ok(s);
        }
        let View::TypeDef { rhs, mods, .. } = view(self.ast(), id) else {
            return Err("class of another kind in a library inline body");
        };
        let flags = mods.iter().fold(Flags::default(), |f, &m| f.with(self.tag(m)));
        let mods = mods.to_vec();
        let class = self.u.define(Symbol {
            name: self.name(id),
            kind: Kind::Class,
            owner: Some(owner),
            flags,
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: Some((self.file, id)),
        });
        if !member {
            self.u.unenter(class);
        }
        self.annotate_quoted(class, &mods);
        for k in self.kids(rhs) {
            match self.tag(k) {
                TYPEPARAM | PARAM | VALDEF | DEFDEF | TYPEDEF => {
                    self.define_definition(k, class, true)?;
                }
                _ => {}
            }
        }
        self.local_dummy(class);
        Ok(class)
    }

    /// The local dummy of class `class`, which owns the statements of its template.
    fn local_dummy(&mut self, class: SymbolId) -> SymbolId {
        let name = format!("<local {}>", self.u.symbol(class).name);
        crate::defn::entered(self.u, class, &name, Kind::Val).unwrap_or_else(|| {
            self.u.define(Symbol {
                name,
                kind: Kind::Val,
                owner: Some(class),
                flags: Flags::default(),
                target: None,
                is_extern: false,
                derived: false,
                decls: Vec::new(),
                tree: None,
            })
        })
    }

    /// The definition of the local class or type alias at `id`.
    pub fn type_def(&mut self, id: NodeId) -> Result<TNodeId, Refusal> {
        let sym =
            self.u.defined_at(self.file, id).ok_or("local type without a symbol in a library inline body")?;
        let View::TypeDef { rhs, is_class, .. } = view(self.ast(), id) else {
            return Err("type definition of another kind in a library inline body");
        };
        let name = self.tname(id, true);
        if !is_class {
            let tpt = self.tpt(rhs)?;
            let t = self.bare(T::TypeDef { name, rhs: tpt }, None, Some(sym));
            return Ok(self.positioned(id, t));
        }
        let dummy = self.local_dummy(sym);
        let outer = std::mem::replace(&mut self.owner, dummy);
        let template = self.template(rhs, dummy);
        self.owner = outer;
        let t = self.bare(T::TypeDef { name, rhs: template? }, None, Some(sym));
        Ok(self.positioned(id, t))
    }

    /// The template at `id` of a local class whose local dummy is `dummy`.
    fn template(&mut self, id: NodeId, dummy: SymbolId) -> Result<TNodeId, Refusal> {
        let (mut constr, mut parents, mut body) = (None, Vec::new(), Vec::new());
        for k in self.kids(id) {
            match self.tag(k) {
                TYPEPARAM | PARAM => body.push(self.param(k)?),
                SELFDEF => return Err("self type of a class in a library inline body"),
                DEFDEF if constr.is_none() => constr = Some(self.local_def(k)?),
                VALDEF | DEFDEF => body.push(self.local_def(k)?),
                TYPEDEF => body.push(self.type_def(k)?),
                IMPORT | EXPORT => {}
                tag if constr.is_none() && is_modifier(tag) => {}
                APPLY | TYPEAPPLY | BLOCK | NEW if constr.is_none() => parents.push(self.term(k)?),
                _ if constr.is_none() => parents.push(self.tpt(k)?),
                tag if is_modifier(tag) => {}
                _ => body.push(self.term(k)?),
            }
        }
        let constr = constr.ok_or("class without a constructor in a library inline body")?;
        let (parents, body) = (self.tpd.list(&parents), self.tpd.list(&body));
        let t = self.bare(T::Template { constr, parents, self_: None, body }, None, Some(dummy));
        Ok(self.positioned(id, t))
    }

    /// The parents of local class `c` as one type, if they do not refer to `locals` (dotty's
    /// `avoid` of a local class reference).
    pub fn class_parents_avoiding(&mut self, tpe: TypeId, locals: &[SymbolId]) -> Option<TypeId> {
        let Type::Sym(None, c) = *self.u.types().get(tpe) else { return None };
        if !locals.contains(&c) || self.u.symbol(c).kind != Kind::Class {
            return None;
        }
        let parents = self.u.parents(c);
        let (&first, rest) = parents.split_first()?;
        let joined = rest.iter().fold(first, |a, &b| self.u.intern_type(Type::And(a, b)));
        let mut found = false;
        self.u.map_type(joined, &mut |u, part| {
            found |= matches!(*u.types().get(part), Type::Sym(_, x) if locals.contains(&x));
            None
        });
        (!found).then_some(joined)
    }
}
