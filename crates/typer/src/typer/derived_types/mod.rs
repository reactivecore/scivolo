// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/Desugar.scala
//   compiler/src/dotty/tools/dotc/ast/untpd.scala

mod params;

use super::{Pt, Typer};
use crate::ctx::{Ctx, Level, Levels};
use crate::defn::type_ref;
use crate::tpd::TNodeId;
use syntax::ast::tree::DerivedKind;
use syntax::ast::{NodeId, TName, Tree};
use tasty::{Kind, TypeId, Universe};

/// The type of one type tree the desugaring leaves to the typer (dotty's `DerivedTypeTree`): the
/// copy of a class's constructor parameter in its `copy` method or companion `apply`, of an enum
/// class's type parameter in a case, the parameter of a variable's setter, and references to a
/// class or its companion, as `case class C[A](x: A)` whose `apply[A](x: A)` copies `A`.
pub struct DerivedTypeTree {
    /// The context the tree is typed in.
    ctx: Ctx,
}

impl Typer<'_> {
    /// Types derived type tree `id` of kind `kind`, which copies or refers to the definition
    /// named `watched`, in `ctx`.
    pub fn typed_derived(
        &mut self,
        id: NodeId,
        kind: DerivedKind,
        watched: Option<TName>,
        ctx: Ctx,
    ) -> TNodeId {
        let Some(watched) = watched else { return self.unsupported(id, "derived type without a source") };
        if kind == DerivedKind::TermRef {
            let ident = self.ast.node(Tree::Ident { name: TName { is_type: false, ..watched } });
            self.ast.with_span(ident, self.ast.span(id));
            let outside = outside_class(&self.levels, self.u, ctx);
            return self.typed(ident, Pt::Any, outside);
        }

        let text = self.text(watched.name).to_owned();
        let derived = DerivedTypeTree { ctx };
        let tpe = match kind {
            DerivedKind::FromParam => derived.param_type(self, &text, watched.is_type),
            DerivedKind::FromEnumParam => derived.enum_param_type(self, &text),
            DerivedKind::SetterParam => derived.setter_param_type(self, &text),
            _ => derived.class_type(self, &text),
        };
        let Some(tpe) = tpe else { return self.unsupported(id, &format!("derived type {kind:?}")) };
        let span = self.ast.span(id);
        self.type_tree(tpe, span, false)
    }
}

impl DerivedTypeTree {
    /// The type of the variable `name` whose setter's parameter is being typed.
    fn setter_param_type(&self, t: &mut Typer<'_>, name: &str) -> Option<TypeId> {
        let setter = match t.u.symbol(self.ctx.owner).kind {
            Kind::Param => t.u.symbol(self.ctx.owner).owner?,
            _ => self.ctx.owner,
        };
        let owner = t.u.symbol(setter).owner?;
        let decls = t.u.symbol(owner).decls.clone();
        let var =
            decls.into_iter().find(|&d| t.u.symbol(d).kind == Kind::Val && t.u.symbol(d).name == name)?;
        t.complete(var);
        Some(t.u.info(var))
    }

    /// A reference to the class named `name`; a given class in an inline method completed.
    fn class_type(&self, t: &mut Typer<'_>, name: &str) -> Option<TypeId> {
        let c = t.class_named(name, self.ctx)?;
        let inline_owner = t.u.symbol(self.ctx.owner).flags.has(tasty::tags::INLINE);
        if inline_owner && t.u.symbol(c).flags.has(tasty::tags::GIVEN) {
            t.complete(c);
        }
        Some(type_ref(t.u, c))
    }
}

/// `ctx` with the scope just outside the members of the innermost class around it, where the
/// class's companion is visible unshadowed by its members.
fn outside_class(levels: &Levels, u: &Universe, ctx: Ctx) -> Ctx {
    let mut level = Some(ctx.level);
    while let Some(l) = level {
        let outer = levels.outer(l);
        if let &Level::Members(o) = levels.get(l)
            && u.symbol(o).kind == Kind::Class
            && let Some(outer) = outer
        {
            return Ctx { level: outer, ..ctx };
        }
        level = outer;
    }
    ctx
}
