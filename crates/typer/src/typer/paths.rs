// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/tpd.scala
//! Trees of path types (dotty's `singleton` and `ref` of a term reference).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::Span;
use tasty::{Type, TypeId};

impl Typer<'_> {
    /// A tree of path type `prefix` at `span`: `C.this`, or a reference to a stable term
    /// through its own prefix unless that can be left implicit from `ctx`; `None` if outside the
    /// subset. Without `ctx`, a parameter of a class that is not static is outside it.
    pub fn path_tree(&mut self, prefix: TypeId, span: Span, depth: u32) -> Option<TNodeId> {
        self.path_tree_from(prefix, span, depth, None)
    }

    pub fn path_tree_from(
        &mut self,
        prefix: TypeId,
        span: Span,
        depth: u32,
        ctx: Option<Ctx>,
    ) -> Option<TNodeId> {
        match self.u.types().get(prefix).clone() {
            Type::This(class) => Some(self.build().this(class, span)),
            Type::Sym(p, s) if !self.u.is_type_symbol(s) && self.is_stable_member(s) && depth > 0 => {
                let static_class =
                    |t: &mut Self, c| t.u.class_symbol(c).is_some_and(|cls| t.is_static_owner(cls));
                let elidable = match p.map(|p| self.u.types().get(p).clone()) {
                    None | Some(Type::NoPrefix | Type::Package(_)) => true,
                    Some(Type::This(c)) if self.is_param_or_accessor(s) => match ctx {
                        Some(ctx) => self.term_prefix_is_elidable(p, s, ctx),
                        None => static_class(self, c).then_some(true)?,
                    },
                    Some(Type::This(c)) => static_class(self, c),
                    Some(Type::Sym(_, o)) => {
                        self.u.symbol(o).flags.has(tasty::tags::OBJECT) && self.is_static_term(o)
                    }
                    _ => return None,
                };
                if elidable {
                    return Some(self.build().ident(s, prefix, span));
                }
                let qualifier = self.path_tree_from(p?, span, depth - 1, ctx)?;
                let name = self.build().term_name(s);
                Some(self.tpd.node(T::Select { qualifier, name }, span, Some(prefix), Some(s)))
            }
            _ => None,
        }
    }
}
