// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! Anonymous classes `new P { body }`, typed as `{ final class $anon extends P { body }; new
//! $anon() }` (dotty's `typedNew` of a template).

use super::{Pt, Typer};
use crate::ctx::Ctx;
use crate::tpd::TNodeId;
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
        self.typed_block(block, stats, Some(apply), pt, ctx)
    }

    /// Expected type `pt` as the parent of an anonymous class without parents: fully defined,
    /// free of skolems, a class reference to a class that is not final and not `Any` or `AnyVal`.
    fn parent_from_pt(&mut self, pt: TypeId) -> Option<TypeId> {
        let pt = self.fully_defined(pt).ok()?;
        if super::deskolemize::has_skolem(self.u, pt) {
            return None;
        }
        let tycon = match self.u.types().get(pt).clone() {
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
