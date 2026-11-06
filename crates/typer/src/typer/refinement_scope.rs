// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//   compiler/src/dotty/tools/dotc/typer/Checking.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! The scope of a refined type tree's members, the synthetic `<refinement>` class, and the
//! recursive type a refined type referring to it is closed over as (dotty's `RecType.closeOver`).

use super::Typer;
use crate::ctx::{Ctx, Level};
use crate::tpd::{TNodeId, Tpd, Tree as T, Walk, traverse_up};
use std::ops::ControlFlow;
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// `ctx` seeing the members of refinement class `class` of parent `parent`, its type members
    /// `items` entered ahead; the parent the refinements apply to, a recursive parent unpacked.
    pub fn refinement_scope(
        &mut self,
        class: SymbolId,
        parent: TypeId,
        items: &[NodeId],
        ctx: Ctx,
    ) -> (TypeId, Ctx) {
        let this = crate::defn::owner_prefix(self.u, class);
        let parent = match *self.u.types().get(parent) {
            Type::Rec(binder, p) => self.u.rec_this_replaced(p, &[binder], this),
            _ => parent,
        };
        let mut class_parent = parent;
        while let Type::Refined(p, ..) | Type::Annotated(p, _) = *self.u.types().get(class_parent) {
            class_parent = p;
        }
        self.u.set_parents(class, vec![class_parent]);
        for &r in items {
            let Tree::TypeDef { name, mods, .. } = *self.ast.tree(r) else { continue };
            let text = self.text(name.name).to_owned();
            let flags = self.ast.modifiers(mods).flags.to_type_flags();
            let sym = self.syms.define(self.u, &text, Kind::Type, class, flags);
            let bounds = self.u.intern_type(Type::Bounds(tasty::NOTHING, tasty::ANY));
            self.u.set_info(sym, bounds);
            self.syms.by_tree.insert(r, sym);
        }
        let level = self.levels.push(Some(ctx.level), Level::Members(class));
        (parent, Ctx { level, ..ctx })
    }

    /// Whether `info` of refinement member `sym` is an alias referring to `sym`.
    pub fn aliases_itself(&mut self, sym: SymbolId, info: TypeId) -> bool {
        let Type::Alias(a) = *self.u.types().get(info) else { return false };
        let mut cyclic = false;
        self.u.map_type(a, &mut |u, x| {
            cyclic |= matches!(*u.types().get(x), Type::Sym(_, s) if s == sym);
            None
        });
        cyclic
    }

    /// `t` with `this` of refinement class `class` and its members replaced by the `this` of a
    /// recursive type around it and members selected on that, `t` itself if it has none.
    pub fn closed_refinement(&mut self, class: SymbolId, t: TypeId) -> TypeId {
        let this = crate::defn::owner_prefix(self.u, class);
        let binder = self.syms.fresh_binder();
        let rec_this = self.u.intern_type(Type::RecThis(binder));
        let mut found = false;
        let closed = self.u.map_type(t, &mut |u, part| {
            let replaced = match *u.types().get(part) {
                _ if part == this => rec_this,
                Type::Sym(Some(p), s) if p == this => {
                    let name = u.symbol(s).name.clone();
                    let named = if u.is_type_symbol(s) {
                        Type::TypeRef(rec_this, name)
                    } else {
                        Type::TermRef(rec_this, name)
                    };
                    u.intern_type(named)
                }
                _ => return None,
            };
            found = true;
            Some(replaced)
        });
        if found { self.u.intern_type(Type::Rec(binder, closed)) } else { t }
    }

    /// Warns of each reference in typed refinement `tree` to a member of refinement class `class`
    /// not in `seen`, or to its `this` (dotty's `checkRefinementNonCyclic`); adds the members
    /// `tree` defines to `seen`.
    pub fn check_refinement_refs(&mut self, tree: TNodeId, class: SymbolId, seen: &mut Vec<SymbolId>) {
        let this = crate::defn::owner_prefix(self.u, class);
        let steps = std::cell::RefCell::new(Vec::new());
        let mut enter = |_: &mut Tpd, n: TNodeId, _| {
            steps.borrow_mut().push((true, n));
            Walk::Children
        };
        let mut leave = |_: &mut Tpd, n: TNodeId, _| {
            steps.borrow_mut().push((false, n));
            ControlFlow::Continue(())
        };
        traverse_up(&mut self.tpd, tree, class, &mut enter, &mut leave);
        for (entered, n) in steps.into_inner() {
            let sym = self.tpd.sym(n);
            let what = match self.tpd.tree(n) {
                T::TypeDef { .. } | T::ValDef { .. } | T::DefDef { .. } if !entered => {
                    seen.extend(sym);
                    continue;
                }
                _ if !entered => continue,
                T::This { .. } if self.tpd.tpe(n) == this => "self",
                T::Ident { .. } | T::Select { .. } => match sym {
                    Some(s) if self.u.symbol(s).owner == Some(class) && !seen.contains(&s) => "forward",
                    _ => continue,
                },
                _ => continue,
            };
            let span = self.tpd.span(n);
            self.warning_at(span, format!("{what} reference in refinement is deprecated"));
        }
    }
}
