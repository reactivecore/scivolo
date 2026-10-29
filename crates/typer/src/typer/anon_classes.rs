// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Anonymous classes `new P { body }`, typed as `{ final class $anon extends P { body }; new
//! $anon() }` (dotty's `typedNew` of a template).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::flags as f;
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, Span, TName, Tree};
use tasty::tags::FINAL;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// `new` of template `template` at `id`, against `pt`.
    pub fn typed_anon_class(&mut self, id: NodeId, template: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        if self.enclosing_class(ctx.owner).is_some_and(|c| self.in_this_call_args(c, ctx)) {
            return self.unsupported(id, "anonymous class in the arguments of a self-constructor call");
        }
        let span = self.ast.span(id);
        let start = Span::synthetic(span.start(), span.start());
        if let Tree::Template { constr, .. } = *self.ast.tree(template)
            && let Tree::DefDef { paramss, .. } = *self.ast.tree(constr)
            && self.ast.clause_items(paramss).is_empty()
        {
            let empty = self.ast.list(&[]);
            let clauses = self.ast.clauses(&[empty]);
            if let Tree::DefDef { paramss, .. } = &mut self.ast.get_mut(constr).tree {
                *paramss = clauses;
            }
        }
        if let Tree::Template { constr, parents_or_derived, .. } = *self.ast.tree(template)
            && self.ast.items(parents_or_derived).is_empty()
            && let Pt::Type(p) = pt
            && let Some(parent) = self.parent_from_pt(p)
        {
            let at = self.ast.span(constr).start();
            let tpt = self.type_tree(parent, Span::synthetic(at, at), false);
            let parent = self.spliced(tpt);
            let parents = self.ast.list(&[parent]);
            if let Tree::Template { parents_or_derived, .. } = &mut self.ast.get_mut(template).tree {
                *parents_or_derived = parents;
            }
        }
        if let Pt::Type(p) = pt {
            self.infer_parent_args(template, p, ctx);
        }
        let name = TName { name: self.name("$anon"), is_type: true };
        let mods = self.ast.mods(Modifiers::with_flags(f::FINAL.or(f::SYNTHETIC)));
        let class = self.ast.node(Tree::TypeDef { name, rhs: template, mods });
        let template_span = self.ast.span(template);
        self.ast.with_span(class, Span::synthetic(template_span.start(), template_span.end()));
        let ident = self.ast.node(Tree::Ident { name });
        self.ast.with_span(ident, start);
        let new = self.ast.node(Tree::New { tpt: ident });
        self.ast.with_span(new, start);
        let init = TName { name: self.name("<init>"), is_type: false };
        let select = self.ast.node(Tree::Select { qualifier: new, name: init });
        self.ast.with_span(select, start);
        let args = self.ast.list(&[]);
        let apply = self.ast.node(Tree::Apply { fun: select, args });
        self.ast.with_span(apply, span);
        let stats = self.ast.list(&[class]);
        let block = self.ast.node(Tree::Block { stats, expr: Some(apply) });
        self.ast.with_span(block, span);
        let open = |s: &mut Self, p: TypeId| {
            let p = s.zonk(p);
            let var = |s: &Self, t: TypeId| matches!(s.u.types().get(t), Type::Var(_));
            match *s.u.types().get(p) {
                Type::Or(a, b, _) => var(s, a) || var(s, b),
                _ => var(s, p),
            }
        };
        let block_pt = match pt {
            Pt::Type(p) if open(self, p) => Pt::Any,
            _ => pt,
        };
        self.typed_block(block, stats, Some(apply), block_pt, ctx)
    }

    /// Each parent of `template` that names a type constructor of the class of `pt`, applied
    /// to `pt`'s arguments (dotty's `inferTypeParams` for an expected type of the parent's class).
    fn infer_parent_args(&mut self, template: NodeId, pt: TypeId, ctx: Ctx) {
        let Tree::Template { parents_or_derived, .. } = *self.ast.tree(template) else { return };
        let Some(pt_class) = self.u.class_symbol(pt) else { return };
        let pt_name = self.u.symbol(pt_class).name.clone();
        let mut parents = self.ast.items(parents_or_derived).to_vec();
        let mut changed = false;
        for p in parents.iter_mut() {
            let (Tree::Ident { name } | Tree::Select { name, .. }) = *self.ast.tree(*p) else { continue };
            if self.text(name.name) != pt_name {
                continue;
            }
            let typed = self.typed_type(*p, ctx);
            let tpe = self.tpd.tpe(typed);
            if self.type_arity(tpe) == 0 || self.u.class_symbol(tpe) != Some(pt_class) {
                continue;
            }
            let base = self.u.base_type(pt, pt_class);
            let Some(Ok(applied)) = base.map(|b| self.fully_defined(b)) else { continue };
            let Type::Applied(_, args) = self.u.types().get(applied).clone() else { continue };
            let span = self.ast.span(*p);
            let Ok(lambda) = self.eta_expanded(pt_class, tpe) else { continue };
            let typed = match *self.tpd.tree(typed) {
                T::Ident { name } => {
                    self.tpd.node(T::Ident { name }, self.tpd.span(typed), Some(lambda), None)
                }
                _ => typed,
            };
            let args: Vec<TNodeId> = args.iter().map(|&a| self.type_tree(a, span, true)).collect();
            let args = self.tpd.list(&args);
            let tpt = self.tpd.node(
                T::AppliedTypeTree { tpt: typed, args },
                span.to_synthetic(),
                Some(applied),
                None,
            );
            *p = self.spliced(tpt);
            changed = true;
        }
        if changed {
            let parents = self.ast.list(&parents);
            if let Tree::Template { parents_or_derived, .. } = &mut self.ast.get_mut(template).tree {
                *parents_or_derived = parents;
            }
        }
    }

    /// Expected type `pt` as the parent of an anonymous class without parents: fully defined,
    /// free of skolems, a class reference to a class that is not final and not `Any` or `AnyVal`.
    fn parent_from_pt(&mut self, pt: TypeId) -> Option<TypeId> {
        let pt = self.fully_defined(pt).ok()?;
        if super::deskolemize::has_skolem(self.u, pt) {
            return None;
        }
        let tycon = match *self.u.types().get(pt) {
            Type::Applied(tycon, _) => tycon,
            _ => pt,
        };
        if !matches!(self.u.types().get(tycon), Type::Sym(..) | Type::TypeRef(..)) {
            return None;
        }
        let class = self.u.class_symbol(pt)?;
        let b = self.u.builtins();
        let top = class == b.any || Some(class) == b.any_val;
        (self.u.symbol(class).kind == tasty::Kind::Class && !self.u.symbol(class).flags.has(FINAL) && !top)
            .then_some(pt)
    }
}
