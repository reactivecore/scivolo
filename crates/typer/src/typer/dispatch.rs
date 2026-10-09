//! The dispatch over untyped tree classes (dotty's `typedUnadapted`).

use super::{Pt, Typer};
use crate::ctx::{Ctx, Mode};
use crate::tpd::TNodeId;
use syntax::ast::{NodeId, Tree};

impl Typer<'_> {
    pub fn typed_unadapted(&mut self, id: NodeId, pt: Pt, ctx: Ctx) -> TNodeId {
        match self.ast.tree(id).clone() {
            Tree::Ident { name } => self.typed_ident(id, name, pt, ctx),
            Tree::Select { qualifier, name } => self.typed_select(id, qualifier, name, pt, ctx),
            Tree::Apply { fun, args } if ctx.mode.is(Mode::PATTERN) => {
                self.typed_unapply(id, fun, args, pt, ctx)
            }
            Tree::Apply { fun, args } if self.is_right_assoc_call(id) => {
                let call = self.typed_apply(id, fun, args, pt, ctx);
                self.lifted_right_assoc(call, ctx)
            }
            Tree::Apply { fun, args } => self.typed_apply(id, fun, args, pt, ctx),
            Tree::Match { selector, cases, kind } => self.typed_match(id, selector, cases, kind, pt, ctx),
            Tree::Alternative { trees } => self.typed_alternative(id, trees, pt, ctx),
            Tree::TypeApply { fun, args } => self.typed_type_apply(id, fun, args, ctx),
            Tree::Literal { value } => self.typed_literal(id, value),
            Tree::Number { digits, kind } => self.typed_number(id, (&digits, kind), pt, ctx),
            Tree::TypedSplice { sym } => self.typed_full_ref(id, sym),
            Tree::This { qual } => self.typed_this(id, qual, ctx),
            Tree::New { tpt } if matches!(self.ast.tree(tpt), Tree::Template { .. }) => {
                self.typed_anon_class(id, tpt, pt, ctx)
            }
            Tree::New { tpt } => self.typed_new(id, tpt, ctx),
            Tree::Typed { expr, tpt } => self.typed_typed(id, expr, tpt, pt, ctx),
            Tree::Block { stats, expr } => self.typed_block(id, stats, expr, pt, ctx),
            Tree::Try { expr, cases, finalizer } => {
                let cases = self.ast.items(cases).to_vec();
                self.typed_try(id, expr, &cases, finalizer, pt, ctx)
            }
            Tree::ParsedTry { expr, handler, finalizer } => {
                self.typed_parsed_try(id, expr, handler, finalizer, pt, ctx)
            }
            Tree::If { cond, thenp, elsep, inline: false } => self.typed_if(id, cond, thenp, elsep, pt, ctx),
            Tree::WhileDo { cond, body } => self.typed_while(id, cond, body, ctx),
            Tree::Assign { lhs, rhs } => self.typed_assign(id, lhs, rhs, ctx),
            Tree::Return { expr } => self.typed_return(id, expr, ctx),
            Tree::Throw { expr } => self.typed_throw(id, expr, ctx),
            Tree::Bind { name, body, .. } => self.typed_bind(id, name, body, pt, ctx),
            Tree::Function { args, body, mods: None, .. } => self.typed_function(id, args, body, pt, ctx),
            Tree::TypeTree
            | Tree::AppliedTypeTree { .. }
            | Tree::SingletonTypeTree { .. }
            | Tree::TypeBoundsTree { .. }
            | Tree::DerivedTypeTree { .. } => self.typed_type(id, ctx),
            Tree::ValDef { .. } => self.typed_val_def(id, ctx),
            Tree::DefDef { .. } => self.typed_def_def(id, ctx),
            Tree::TypeDef { rhs, .. } if matches!(self.ast.tree(rhs), Tree::Template { .. }) => {
                self.typed_class_def(id, ctx)
            }
            Tree::TypeDef { .. } => self.typed_type_def(id, ctx),
            Tree::PackageDef { pid, stats } => self.typed_package_def(id, pid, stats, ctx),
            Tree::Import { selectors, .. } if self.import_levels.contains_key(&id) => {
                self.typed_import(id, selectors)
            }
            t => {
                let what = format!("{t:?}");
                self.unsupported(id, what.split([' ', '{']).next().unwrap_or("tree"))
            }
        }
    }
}
