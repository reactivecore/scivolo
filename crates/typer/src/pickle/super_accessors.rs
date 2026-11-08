// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SuperAccessors.scala
//! The super accessors dotty's `PostTyper` gives a top-level trait for its calls `super.m`
//! (`superAccessorCall`): declared without a body, each call a selection of its accessor on the
//! trait's `this`.

use super::{Refusal, TreePickler};
use crate::defn::type_ref;
use crate::signatures::result_sig;
use crate::tpd::{TNodeId, Tree as T};
use tasty::tags::{ARTIFACT, DEFDEF, EMPTYCLAUSE, INLINE, PARAM, SELECT, SELECTin, SPLITCLAUSE};
use tasty::write::Name;
use tasty::{Flags, Kind, Symbol, SymbolId, Type};

/// Super accessors pickled, each its class and name.
pub type Accessors = Vec<(SymbolId, String)>;

impl TreePickler<'_> {
    /// The super accessors of trait `class` for its calls `super.m` in `stats` outside nested
    /// classes, in the order dotty's `PostTyper` meets them, each call recorded to select its
    /// accessor.
    pub fn trait_super_accessors(
        &mut self,
        class: SymbolId,
        stats: &[TNodeId],
    ) -> Result<Vec<SymbolId>, Refusal> {
        let mut found: Vec<(SymbolId, SymbolId)> = Vec::new();
        let mut todo: Vec<TNodeId> = stats.iter().rev().copied().collect();
        while let Some(t) = todo.pop() {
            match *self.tpd.tree(t) {
                T::TypeDef { rhs, .. } if matches!(self.tpd.tree(rhs), T::Template { .. }) => continue,
                T::TypeTree { .. } | T::SingletonTypeTree { .. } | T::AppliedTypeTree { .. } => continue,
                T::Apply { fun, args } => {
                    todo.push(fun);
                    todo.extend(self.tpd.items(args).iter().rev());
                    continue;
                }
                T::Select { qualifier, .. } => {
                    if let T::Super { mix, .. } = *self.tpd.tree(qualifier)
                        && self.tpd.sym(qualifier) == Some(class)
                        && matches!(*self.tpd.tree(mix), T::Ident { name } if self.names.text(name.name).is_empty())
                    {
                        self.super_call(t, class, &mut found)?;
                    }
                }
                _ => {}
            }
            todo.extend(self.tpd.children(t).iter().rev());
        }
        Ok(found.into_iter().map(|(_, acc)| acc).collect())
    }

    /// Records the accessor of super call `sel` in trait `class`, made unless `found`, each a
    /// member with its accessor, has it.
    fn super_call(
        &mut self,
        sel: TNodeId,
        class: SymbolId,
        found: &mut Vec<(SymbolId, SymbolId)>,
    ) -> Result<(), Refusal> {
        let Some(member) = self.tpd.sym(sel) else { return Ok(()) };
        let m = self.u.symbol(member);
        if m.kind.is_type() || m.flags.has(INLINE) {
            return Ok(());
        }
        if m.target.is_some() {
            return Err("a super call of a method with a target name in a trait");
        }
        let acc = match found.iter().find(|&&(m, _)| m == member) {
            Some(&(_, acc)) => acc,
            None => {
                let acc = self.new_accessor(class, member, sel)?;
                found.push((member, acc));
                acc
            }
        };
        self.super_calls.insert(sel, acc);
        Ok(())
    }

    fn new_accessor(&mut self, class: SymbolId, member: SymbolId, sel: TNodeId) -> Result<SymbolId, Refusal> {
        let mut prefix = vec![self.u.symbol(class).name.clone()];
        let mut owner = self.u.symbol(class).owner.ok_or("a trait without owner")?;
        while owner != self.u.root() {
            if self.u.symbol(owner).kind != Kind::Package {
                return Err("a super call in a trait that is not top-level");
            }
            prefix.push(self.u.symbol(owner).name.clone());
            owner = self.u.symbol(owner).owner.ok_or("a package without owner")?;
        }
        let original = self.u.symbol(member).name.clone();
        let name = format!("{}$$super${original}", self.u.expanded_prefix(class));
        let flags = Flags::default().with(ARTIFACT);
        let (kind, target, decls) = (Kind::Def, None, Vec::new());
        let symbol = Symbol {
            name: name.clone(),
            kind,
            owner: None,
            flags,
            target,
            is_extern: false,
            derived: false,
            decls,
            tree: None,
        };
        let acc = self.u.define(symbol);
        self.u.set_owner(acc, class);
        let info = self.u.widen(self.tpd.tpe(sel));
        self.u.set_info(acc, info);
        let mut parts = prefix.iter().rev().map(|p| Name::utf8(p));
        let first = parts.next().expect("the trait's name");
        let expanded = parts.fold(first, |q, p| Name::ExpandPrefix(Box::new(q), Box::new(p)));
        let expanded = Name::Expanded(Box::new(expanded), Box::new(Name::utf8(&original)));
        self.accessor_names.insert(acc, Name::SuperAccessor(Box::new(expanded)));
        self.accessors.push((class, name));
        Ok(acc)
    }

    /// The name of super accessor `s`, if it is one.
    pub fn accessor_name(&self, s: SymbolId) -> Option<Name> {
        self.accessor_names.get(&s).cloned()
    }

    /// Super accessor `acc`, declared without a body (dotty's `DefDef(acc, EmptyTree)`).
    pub fn super_accessor_def(&mut self, acc: SymbolId) -> Result<(), Refusal> {
        self.defs.entry(acc).or_insert(None);
        self.register_def(acc);
        let name = self.accessor_name(acc).ok_or("a super accessor without a name")?;
        let info = self.u.info(acc);
        self.with_length(DEFDEF, &mut |p| {
            p.name(&name);
            let (mut t, mut term_before) = (info, false);
            while let Type::Method { params, result, implicit, contextual, .. } = p.u.types().get(t).clone() {
                if implicit || contextual {
                    return Err("a super call of a method with implicit parameters in a trait");
                }
                if params.is_empty() {
                    p.byte(EMPTYCLAUSE);
                } else if term_before {
                    p.byte(SPLITCLAUSE);
                }
                for (name, tpe) in params.iter() {
                    p.with_length(PARAM, &mut |p| {
                        p.name(&Name::utf8(name));
                        p.ty(*tpe, false)
                    })?;
                }
                (t, term_before) = (result, !params.is_empty());
            }
            if matches!(p.u.types().get(t), Type::Poly { .. }) {
                return Err("a super call of a polymorphic method in a trait");
            }
            p.ty(t, false)?;
            p.byte(ARTIFACT);
            Ok(())
        })
    }

    /// Super call `sel`'s selection of accessor `acc` on `this` of trait `class`.
    pub fn super_accessor_select(&mut self, sel: TNodeId, acc: SymbolId) -> Result<(), Refusal> {
        let class = self.u.symbol(acc).owner.ok_or("a super accessor without owner")?;
        let class = type_ref(self.u, class);
        if result_sig(self.u, acc).is_none() {
            self.byte(SELECT);
            self.signed_name(acc, false)?;
            return self.class_this(sel, None, class);
        }
        self.with_length(SELECTin, &mut |p| {
            p.signed_name(acc, true)?;
            p.class_this(sel, None, class)?;
            p.ty(class, false)
        })
    }
}
