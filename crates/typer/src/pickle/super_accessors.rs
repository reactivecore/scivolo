// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/SuperAccessors.scala
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! The super accessors dotty's `PostTyper` gives a class `C` for its calls `C.super[M].m`
//! (`transformSuperSelect`, `superAccessorCall`): in a trait without `M`, or in a class nested in
//! `C`; declared without a body, each call a selection of its accessor on `C.this`.

use super::names::expanded_prefix_name;
use super::{Refusal, TreePickler};
use crate::defn::type_ref;
use crate::protected_access::super_accessor_needed;
use crate::signatures::result_sig;
use crate::tpd::{TNodeId, Tree as T};
use names::name_kinds;
use syntax::ast::Span;
use tasty::tags::{ARTIFACT, INLINE, MACRO, SELECT, SELECTin};
use tasty::write::Name;
use tasty::{Flags, Kind, Symbol, SymbolId, Universe};

/// Super accessors pickled, each its class and name.
pub type Accessors = Vec<(SymbolId, String)>;

/// A super call's member and mixin, and its accessor.
type Found = Vec<((SymbolId, String), SymbolId)>;

impl TreePickler<'_> {
    /// The super accessors of class `class` for its calls `super.m` in `stats` and the classes
    /// nested in them, in the order dotty's `PostTyper` meets them, each call recorded to select
    /// its accessor.
    pub fn super_accessors(&mut self, class: SymbolId, stats: &[TNodeId]) -> Result<Vec<SymbolId>, Refusal> {
        let mut found: Found = Vec::new();
        let mut todo: Vec<(TNodeId, SymbolId, bool)> =
            stats.iter().rev().map(|&s| (s, class, false)).collect();
        while let Some((t, current, invalid)) = todo.pop() {
            let (mut inner, mut inner_invalid) = (current, invalid);
            match *self.tpd.tree(t) {
                T::TypeDef { rhs, .. } if matches!(self.tpd.tree(rhs), T::Template { .. }) => {
                    (inner, inner_invalid) = (self.tpd.sym(t).unwrap_or(current), false);
                }
                T::DefDef { .. } if self.tpd.sym(t).is_some_and(|m| self.has_extension(m)) => {
                    inner_invalid = true
                }
                T::TypeTree { .. } | T::SingletonTypeTree { .. } | T::AppliedTypeTree { .. } => continue,
                T::Apply { fun, args } => {
                    todo.push((fun, current, invalid));
                    todo.extend(self.tpd.items(args).iter().rev().map(|&a| (a, current, invalid)));
                    continue;
                }
                T::Select { qualifier, .. } => {
                    if let T::Super { mix, .. } = *self.tpd.tree(qualifier)
                        && self.tpd.sym(qualifier) == Some(class)
                        && let T::Ident { name } = *self.tpd.tree(mix)
                    {
                        let mix = self.names.text(name.name).to_owned();
                        if current != class || invalid || mix.is_empty() && self.u.is_trait(class) {
                            self.super_call(t, class, mix, &mut found)?;
                        }
                    } else if matches!(self.tpd.tree(qualifier), T::This { .. })
                        && self.tpd.sym(qualifier) == Some(class)
                        && let Some(member) = self.tpd.sym(t)
                        && super_accessor_needed(self.u, member, current)?
                    {
                        self.super_call(t, class, String::new(), &mut found)?;
                    }
                }
                _ => {}
            }
            todo.extend(self.tpd.children(t).iter().rev().map(|&c| (c, inner, inner_invalid)));
        }
        Ok(found.into_iter().map(|(_, acc)| acc).collect())
    }

    /// Whether method `m` gets an extension method (dotty's `isMethodWithExtension`).
    fn has_extension(&mut self, m: SymbolId) -> bool {
        let s = self.u.symbol(m);
        let Some(owner) = s.owner.filter(|_| s.kind == Kind::Def) else { return false };
        !self.u.is_constructor(m)
            && !self.accessor_names.contains_key(&m)
            && !s.flags.has(INLINE)
            && !s.flags.has(MACRO)
            && !self.u.is_trait(owner)
            && self.u.symbol(owner).kind == Kind::Class
            && self.u.underlying(owner).is_some()
    }

    /// Records the accessor of super call `sel` in class `class` with mixin `mix`, made unless
    /// `found` has it.
    fn super_call(
        &mut self,
        sel: TNodeId,
        class: SymbolId,
        mix: String,
        found: &mut Found,
    ) -> Result<(), Refusal> {
        let Some(member) = self.tpd.sym(sel) else { return Ok(()) };
        let m = self.u.symbol(member);
        if m.kind.is_type() || m.flags.has(INLINE) {
            return Ok(());
        }
        let key = (member, mix);
        let acc = match found.iter().find(|(k, _)| *k == key) {
            Some(&(_, acc)) => acc,
            None => {
                let acc = self.new_accessor(class, &key, sel);
                found.push((key, acc));
                acc
            }
        };
        self.super_calls.insert(sel, acc);
        Ok(())
    }

    /// The accessor of `class` for super call `sel` of `member` with mixin `mix`, named
    /// `super$m` or `m$super$M`, a trait's expanded by its prefix (dotty's `superAccessorName`).
    fn new_accessor(
        &mut self,
        class: SymbolId,
        (member, mix): &(SymbolId, String),
        sel: TNodeId,
    ) -> SymbolId {
        let original = self.u.symbol(*member).name.clone();
        let trait_ = self.u.is_trait(class);
        let accessor_name = |u: &Universe, original: &str| {
            let unexpanded = name_kinds::super_accessor(original, mix);
            match trait_ {
                true => name_kinds::expanded(&u.expanded_prefix(class), &unexpanded),
                false => unexpanded,
            }
        };
        let name = accessor_name(self.u, &original);
        let target = self.u.symbol(*member).target.clone().map(|t| accessor_name(self.u, &t));
        let symbol = Symbol {
            name: name.clone(),
            kind: Kind::Def,
            owner: None,
            flags: Flags::default().with(ARTIFACT),
            target,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        };
        let acc = self.u.define(symbol);
        self.u.set_owner(acc, class);
        let info = self.u.widen(self.tpd.tpe(sel));
        self.u.set_info(acc, info);
        let tasty_name = |u: &Universe, original: &str| {
            let base = match trait_ {
                true => {
                    Name::Expanded(Box::new(expanded_prefix_name(u, class)), Box::new(Name::utf8(original)))
                }
                false => Name::utf8(original),
            };
            let unexpanded = match mix.as_str() {
                "" => base,
                mix => Name::ExpandPrefix(Box::new(base), Box::new(Name::utf8(mix))),
            };
            Name::SuperAccessor(Box::new(unexpanded))
        };
        self.accessor_names.insert(acc, tasty_name(self.u, &original));
        let mut defined = Span::NONE;
        if let Some(t) = self.u.symbol(*member).target.clone() {
            self.accessor_targets.insert(acc, tasty_name(self.u, &t));
            let def = (0..self.tpd.len())
                .map(TNodeId::new)
                .find(|&t| matches!(self.tpd.tree(t), T::DefDef { .. }) && self.tpd.sym(t) == Some(*member));
            defined = def.map_or(Span::NONE, |d| Span::at(self.tpd.span(d).point()));
        }
        self.accessor_spans.insert(acc, (Span::at(self.tpd.span(sel).point()), defined));
        self.accessors.push((class, name));
        acc
    }

    /// The name of super accessor `s`, if it is one.
    pub fn accessor_name(&self, s: SymbolId) -> Option<Name> {
        self.accessor_names.get(&s).cloned()
    }

    /// Super call `sel`'s selection of accessor `acc` on `this` of its class.
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
