//! The capture sets an argument gives an inferred type argument: dotty's subtyping drops the one
//! at a lower bound's top, `ConstraintHandling.widenInferred` keeps a singleton's.

use super::TreePickler;
use super::captured_exprs::{Captured, captured, strip_annotations};
use super::captured_type_args::has_part;
use crate::sealed::is_contained_in;
use crate::tpd::{TNodeId, Tree as T};
use crate::typer::seen_from::resolve_class_params;
use tasty::{SymbolId, TypeId};

/// Accepts the type parameter an argument is inferred for.
type IsParam<'a> = &'a dyn Fn(&tasty::Universe, TypeId) -> bool;

/// Whether an expression's type is a singleton dotty widens at instantiation.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Path {
    /// A path, kept as a lower bound.
    Kept,
    /// Not a path, or one local to the closure the type argument is inferred outside of.
    Dropped,
}

impl TreePickler<'_> {
    /// Determines what argument `arg` of type `c` gives the type parameter that is its
    /// parameter's type, as a lower bound of a type variable: a path (`id(c)`, `id(c.iterator)`)
    /// its capture sets, other expressions (`id(mk(c))`, `id(id(b))`) none at the top. A path is
    /// one only outside closure method `local`. `None` for branches mixing paths and other
    /// expressions, a block ending in a path, and a path's set when type parameter `param` occurs
    /// other than covariantly in the method's result `result` (an expected type bounds it below).
    pub fn widened_arg(
        &mut self,
        (arg, c): (TNodeId, Captured),
        local: Option<SymbolId>,
        (result, param): (TypeId, IsParam<'_>),
    ) -> Option<Captured> {
        let Captured::With(t) = c else { return Some(c) };
        match self.path(arg, local)? {
            Path::Kept => {
                let result = resolve_class_params(self.u, result);
                only_covariant(self.u, result, param).then_some(c)
            }
            Path::Dropped => Some(captured(self.u, strip_annotations(self.u, t))),
        }
    }

    /// Whether expression `e` is a path outside closure method `local`.
    fn path(&self, e: TNodeId, local: Option<SymbolId>) -> Option<Path> {
        match *self.tpd.tree(e) {
            T::Ident { .. } | T::Select { .. } => {
                let s = self.tpd.sym(e)?;
                let local_root =
                    self.root_sym(e).is_some_and(|r| local.is_some_and(|m| is_contained_in(self.u, r, m)));
                Some(match self.u.symbol(s).kind.is_term() && !local_root {
                    true => Path::Kept,
                    false => Path::Dropped,
                })
            }
            T::This { .. } => Some(Path::Kept),
            T::Block { stats, expr } => match self.path(expr, local)? {
                Path::Kept if !self.tpd.items(stats).is_empty() => None,
                p => Some(p),
            },
            T::If { thenp, elsep, .. } => self.branches_path(&[thenp, elsep], local),
            T::Match { cases, .. } => {
                let bodies: Vec<TNodeId> = (self.tpd.items(cases).iter())
                    .filter_map(|&c| match *self.tpd.tree(c) {
                        T::CaseDef { body, .. } => Some(body),
                        _ => None,
                    })
                    .collect();
                self.branches_path(&bodies, local)
            }
            _ => Some(Path::Dropped),
        }
    }

    /// The path kind all `branches` but those of type `Nothing` share.
    fn branches_path(&self, branches: &[TNodeId], local: Option<SymbolId>) -> Option<Path> {
        let mut found = None;
        for &b in branches.iter().filter(|&&b| self.tpd.get(b).tpe != Some(tasty::NOTHING)) {
            let p = self.path(b, local)?;
            if found.is_some_and(|f| f != p) {
                return None;
            }
            found = Some(p);
        }
        found
    }

    /// The symbol of the identifier a chain of selections `e` starts with.
    fn root_sym(&self, mut e: TNodeId) -> Option<SymbolId> {
        while let T::Select { qualifier, .. } = *self.tpd.tree(e) {
            e = qualifier;
        }
        match self.tpd.tree(e) {
            T::Ident { .. } => self.tpd.sym(e),
            _ => None,
        }
    }
}

/// Whether the parts of `t` that `param` accepts occur only covariantly in it.
fn only_covariant(
    u: &mut tasty::Universe,
    t: TypeId,
    param: &dyn Fn(&tasty::Universe, TypeId) -> bool,
) -> bool {
    let t = strip_annotations(u, t);
    if param(u, t) || !has_part(u, t, param) {
        return true;
    }
    let Some((tycon, args)) = u.applied(t).map(|(c, a)| (c, a.to_vec())) else { return false };
    let variances = u.tycon_variances(tycon, args.len());
    args.iter().zip(variances).all(|(&a, v)| !has_part(u, a, param) || (v > 0 && only_covariant(u, a, param)))
}
