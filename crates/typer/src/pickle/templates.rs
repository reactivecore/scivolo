// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//! Class templates (dotty's `Template` case of `pickleTree`, `decomposeTemplateBody`).

use super::names::sym_name;
use super::{Refusal, TreePickler};
use crate::tpd::{TNodeId, Tree as T};
use tasty::tags::{IDENTtpt, PARAMsetter, QUALTHIS, SELFDEF, SUPER, SYNTHETIC, TEMPLATE};
use tasty::write::Name;
use tasty::{Kind, Type, TypeId};

impl TreePickler<'_> {
    /// Template `id`: its parameters, parents, self and the constructor before the other
    /// statements.
    pub fn template(&mut self, id: TNodeId) -> Result<(), Refusal> {
        let sym = self.tpd.sym(id).ok_or("a template without a symbol")?;
        let T::Template { constr, parents, self_, body } = *self.tpd.tree(id) else {
            return Err("a template of another kind");
        };
        if !matches!(self.defs.get(&sym), Some(Some(_))) {
            self.register_def(sym);
        }
        let (params, rest): (Vec<TNodeId>, Vec<TNodeId>) =
            self.tpd.items(body).iter().partition(|&&s| self.is_class_param(s));
        let mut classes = Vec::new();
        let mut parents = self.tpd.items(parents).to_vec();
        parents.retain(|&p| {
            let class = self.u.class_symbol(self.tpd.tpe(p));
            let again = class.is_some() && classes.contains(&class);
            classes.push(class);
            !again
        });
        let stats: Vec<TNodeId> = std::iter::once(constr).chain(rest).collect();
        let class = match self.u.is_class(sym) {
            true => Some(sym),
            false => self.u.symbol(sym).owner,
        };
        let accessors = match class.filter(|_| self.file.is_some()) {
            Some(class) => self.super_accessors(class, &stats)?,
            None => Vec::new(),
        };
        self.with_length(TEMPLATE, &mut |p| {
            p.params(&params)?;
            parents.iter().try_for_each(|&t| p.tree(t))?;
            if let Some(s) = self_ {
                p.self_def(s)?;
            }
            p.pre_register(&stats);
            p.tree(constr)?;
            let synthetic = stats[1..].iter().take_while(|&&s| p.is_synthetic_def(s)).count() + 1;
            stats[1..synthetic].iter().try_for_each(|&t| p.tree(t))?;
            accessors.iter().try_for_each(|&a| p.super_accessor_def(a))?;
            stats[synthetic..].iter().try_for_each(|&t| p.tree(t))
        })
    }

    /// Whether statement `s` of a template is a synthetic method, as those dotty's
    /// `SyntheticMembers` puts before the super accessors.
    fn is_synthetic_def(&self, s: TNodeId) -> bool {
        let synthetic = self.tpd.sym(s).is_some_and(|m| self.u.symbol(m).flags.has(SYNTHETIC));
        synthetic && matches!(self.tpd.tree(s), T::DefDef { .. })
    }

    /// Whether statement `s` of a template is a type parameter or a parameter accessor other
    /// than a setter.
    fn is_class_param(&self, s: TNodeId) -> bool {
        let Some(sym) = self.tpd.sym(s) else { return false };
        let sym = self.u.symbol(sym);
        match self.tpd.tree(s) {
            T::TypeDef { .. } => sym.kind == Kind::TypeParam,
            T::ValDef { .. } | T::DefDef { .. } => sym.flags.has(PARAMsetter) && !sym.name.ends_with("_="),
            _ => false,
        }
    }

    /// `this` of type `tpe`, `qual` its class's name, empty if not written.
    pub fn this(&mut self, qual: TNodeId, tpe: TypeId) -> Result<(), Refusal> {
        let T::Ident { name } = *self.tpd.tree(qual) else { return Err("a this of another kind") };
        if self.names.text(name.name).is_empty() {
            return self.ty(tpe, false);
        }
        let Type::This(class) = *self.u.types().get(tpe) else { return Err("a this of another type") };
        self.class_this(qual, Some(qual), class)
    }

    /// `C.this` of class type `class` for tree `id`, `qual` the tree of its name if written;
    /// without one, as PostTyper makes reference `id` to a self (dotty's `This(cls)`).
    pub fn class_this(&mut self, id: TNodeId, qual: Option<TNodeId>, class: TypeId) -> Result<(), Refusal> {
        let name = match qual.map(|q| self.tpd.tree(q)) {
            Some(&T::Ident { name }) => Name::utf8(self.names.text(name.name)),
            _ => {
                let cls = self.u.class_symbol(class).ok_or("a this of no class")?;
                sym_name(self.u, cls)
            }
        };
        self.byte(QUALTHIS);
        match qual {
            Some(q) => _ = self.buf.register_tree(q.index() as u32),
            None if self.super_calls.contains_key(&id) => {
                self.synthetic_here(self.tpd.span(id).start_pos().to_synthetic(), Some(id))
            }
            None => _ = self.self_refs.insert(id),
        }
        self.byte(IDENTtpt);
        self.name(&name);
        self.ty(class, false)
    }

    /// `qual.super[mix]` of type `tpe`, `mix` empty if not written.
    pub fn super_(&mut self, qual: TNodeId, mix: TNodeId, tpe: TypeId) -> Result<(), Refusal> {
        let T::Ident { name } = *self.tpd.tree(mix) else { return Err("a super of another kind") };
        let name = self.names.text(name.name).to_owned();
        let Type::Super(_, mixin) = *self.u.types().get(tpe) else { return Err("a super of another type") };
        self.with_length(SUPER, &mut |p| {
            p.tree(qual)?;
            if !name.is_empty() {
                p.buf.register_tree(mix.index() as u32);
                p.byte(IDENTtpt);
                p.name(&Name::utf8(&name));
                p.ty(mixin, false)?;
            }
            Ok(())
        })
    }

    /// Shares the class of module val's `new Foo$()` `rhs` with its type `tpt`, one tree in
    /// dotty's `moduleDef`.
    pub fn share_module_class(&mut self, tpt: TNodeId, rhs: Option<TNodeId>) {
        let Some(T::Apply { fun, .. }) = rhs.map(|r| self.tpd.tree(r)) else { return };
        let T::Select { qualifier, .. } = *self.tpd.tree(*fun) else { return };
        if let T::New { tpt: class } = *self.tpd.tree(qualifier) {
            self.shared.insert(class, tpt);
        }
    }

    /// The self definition `s` of a template.
    fn self_def(&mut self, s: TNodeId) -> Result<(), Refusal> {
        let T::ValDef { name, tpt, .. } = *self.tpd.tree(s) else { return Err("a self of another kind") };
        self.byte(SELFDEF);
        self.name(&Name::utf8(self.names.text(name.name)));
        self.tree(tpt)
    }
}
