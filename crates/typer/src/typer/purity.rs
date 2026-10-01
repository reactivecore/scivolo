//! Pure expressions and the warnings about them in statement position (dotty's `exprPurity`,
//! reduced, and `checkStatementPurity`).

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Kind, SymbolId};

/// The classes whose methods do not observe their arguments (dotty's `isKnownPureOp`).
const PURE_OP_OWNERS: [&str; 10] = [
    "scala.Int",
    "scala.Long",
    "scala.Short",
    "scala.Byte",
    "scala.Char",
    "scala.Float",
    "scala.Double",
    "scala.Boolean",
    "scala.Unit",
    "java.lang.String",
];

impl Typer<'_> {
    /// Whether evaluating `tree` has no effect.
    pub fn is_pure_expr(&mut self, tree: TNodeId) -> bool {
        match *self.tpd.tree(tree) {
            T::Literal { .. } | T::This { .. } | T::New { .. } => true,
            T::Ident { .. } => self.is_pure_ref(tree),
            T::Select { qualifier, .. } => self.is_pure_ref(tree) && self.is_pure_expr(qualifier),
            T::Apply { fun, args } => {
                let callee = self.callee(fun);
                let stable_ctor = callee.is_some_and(|s| {
                    self.u.symbol(s).name == "<init>" && self.u.symbol(s).flags.has(tasty::tags::STABLE)
                });
                let pure_op = stable_ctor
                    || self.has_constant_type(tree)
                        && callee.and_then(|s| self.u.symbol(s).owner).is_some_and(|o| {
                            let name = self.u.full_name(o);
                            PURE_OP_OWNERS.contains(&name.as_str())
                        });
                let args = self.tpd.items(args).to_vec();
                pure_op && self.is_pure_expr(fun) && args.into_iter().all(|a| self.is_pure_expr(a))
            }
            T::Block { stats, expr } => self.tpd.items(stats).is_empty() && self.is_pure_expr(expr),
            _ => false,
        }
    }

    fn has_constant_type(&self, tree: TNodeId) -> bool {
        let tpe = self.tpd.get(tree).tpe;
        tpe.is_some_and(|t| matches!(self.u.types().get(t), tasty::Type::Constant(..) | tasty::Type::Str(_)))
    }

    /// Whether reference `tree` has a constant type or refers to a stable symbol.
    fn is_pure_ref(&mut self, tree: TNodeId) -> bool {
        self.has_constant_type(tree) || self.tpd.sym(tree).is_some_and(|s| self.is_stable_ref(s, tree))
    }

    /// Whether reference `tree` to `s` evaluates nothing: a stable value, an object, a package,
    /// a type, or a method not applied yet; a parameterless method is a call.
    fn is_stable_ref(&mut self, s: SymbolId, tree: TNodeId) -> bool {
        let sym = self.u.symbol(s);
        let mutable = sym.flags.has(tasty::tags::MUTABLE) || sym.flags.has(tasty::tags::LAZY);
        match sym.kind {
            Kind::Val | Kind::Param => !mutable,
            Kind::Def => {
                let tpe = self.tpd.tpe(tree);
                let info = self.method_info(tpe).map(|i| self.u.types().get(i).clone());
                matches!(info, Some(tasty::Type::Method { .. } | tasty::Type::Poly { .. }))
            }
            _ => true,
        }
    }

    /// The method a function tree refers to.
    fn callee(&self, fun: TNodeId) -> Option<SymbolId> {
        match *self.tpd.tree(fun) {
            T::Ident { .. } | T::Select { .. } => self.tpd.sym(fun),
            T::TypeApply { fun, .. } => self.callee(fun),
            _ => None,
        }
    }

    /// Warns about pure statement `tree`.
    pub fn check_statement_purity(&mut self, tree: TNodeId) {
        if self.is_pure_expr(tree) {
            let at = self.tpd.span(tree).point();
            self.diags.warning(at, "A pure expression does nothing in statement position");
        }
    }

    /// Warns about pure expression `tree` whose value is discarded for an expected `Unit`.
    pub fn check_unit_purity(&mut self, tree: TNodeId) {
        if self.is_pure_expr(tree) {
            let tpe = self.tpd.tpe(tree);
            let widened = self.u.widen(tpe);
            let shown = self.u.show(widened);
            let msg = format!("Discarded non-Unit value of type {shown}. Add `: Unit` to discard silently.");
            let at = self.tpd.span(tree).point();
            self.diags.warning(at, msg);
        }
    }
}
