//! How far evaluating a tree has effects (dotty's `exprPurity`, `statPurity`, `refPurity`), and
//! the warnings about pure expressions in statement position (`checkStatementPurity`).

use super::Typer;
use crate::tpd::{TNodeId, Tree as T};
use tasty::{Kind, SymbolId, Type};

/// dotty's `PurityLevel` bits: evaluating twice gives the same, evaluating has no effect, the
/// tree is a path; the weaker of two levels is their intersection.
const IDEMPOTENT: u8 = 1;
const PURE: u8 = 2 | IDEMPOTENT;
const IDEMPOTENT_PATH: u8 = 4 | IDEMPOTENT;
const PURE_PATH: u8 = 4 | PURE;
const IMPURE: u8 = 0;

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
        self.expr_purity(tree) & PURE == PURE
    }

    /// Whether evaluating `tree` twice has the effect of evaluating it once.
    pub fn is_idempotent_expr(&mut self, tree: TNodeId) -> bool {
        self.expr_purity(tree) & IDEMPOTENT == IDEMPOTENT
    }

    fn expr_purity(&mut self, tree: TNodeId) -> u8 {
        match self.tpd.tree(tree).clone() {
            T::Literal { .. } | T::ClassLiteral { .. } | T::This { .. } | T::Super { .. } => PURE_PATH,
            T::New { .. } | T::Closure { .. } => PURE,
            T::Ident { .. } => self.ref_purity(tree),
            T::Select { qualifier, .. } => self.ref_purity(tree) & self.expr_purity(qualifier),
            T::TypeApply { fun, .. } if self.is_method_typed(tree) => self.expr_purity(fun),
            T::Apply { fun, args } => {
                let args = self.tpd.items(args).to_vec();
                let factor = args.into_iter().fold(self.expr_purity(fun), |p, a| p & self.expr_purity(a));
                let callee = self.callee(fun);
                if self.is_method_typed(tree) || self.is_pure_apply(tree, callee) {
                    factor & PURE
                } else if callee.is_some_and(|s| self.is_stable_member(s)) {
                    factor & IDEMPOTENT
                } else {
                    IMPURE
                }
            }
            T::Typed { expr, .. } => self.expr_purity(expr),
            T::Block { stats, expr } => {
                let stats = self.tpd.items(stats).to_vec();
                stats.into_iter().fold(self.expr_purity(expr), |p, s| p & self.stat_purity(s))
            }
            _ => IMPURE,
        }
    }

    fn stat_purity(&mut self, s: TNodeId) -> u8 {
        match *self.tpd.tree(s) {
            T::DefDef { .. } | T::TypeDef { .. } | T::Import { .. } => PURE_PATH,
            T::ValDef { rhs, .. } => {
                let mutable =
                    self.tpd.sym(s).is_some_and(|v| self.u.symbol(v).flags.has(tasty::tags::MUTABLE));
                match rhs {
                    _ if mutable => IMPURE,
                    Some(r) => self.expr_purity(r) & PURE,
                    None => PURE,
                }
            }
            _ => IMPURE,
        }
    }

    /// The purity of reference `tree`: a path if it is to a method not applied yet, of a
    /// constant type, or to a stable value; an idempotent path for a lazy value or an object
    /// that runs code when created.
    fn ref_purity(&mut self, tree: TNodeId) -> u8 {
        if self.is_method_typed(tree) || self.has_constant_type(tree) {
            return PURE_PATH;
        }
        let Some(s) = self.tpd.sym(tree) else { return IMPURE };
        if !self.is_stable_member(s) {
            return IMPURE;
        }
        let flags = self.u.symbol(s).flags;
        if flags.has(tasty::tags::OBJECT) && self.u.symbol(s).kind == Kind::Val {
            let no_inits = self.u.module_class(s).is_some_and(|c| self.is_no_inits(c));
            return if no_inits { PURE_PATH } else { IDEMPOTENT_PATH };
        }
        if flags.has(tasty::tags::LAZY) { IDEMPOTENT_PATH } else { PURE_PATH }
    }

    /// Whether `tree` has a method type or refers to a method with parameters, unapplied.
    fn is_method_typed(&mut self, tree: TNodeId) -> bool {
        let tpe = self.tpd.tpe(tree);
        let info = self.method_info(tpe).unwrap_or(tpe);
        matches!(self.u.types().get(info), Type::Method { .. } | Type::Poly { .. })
    }

    /// dotty's `isPureApply`: a constant operation of a primitive class, or a stable constructor.
    fn is_pure_apply(&mut self, tree: TNodeId, callee: Option<SymbolId>) -> bool {
        let Some(s) = callee else { return false };
        if self.u.symbol(s).name == "<init>" {
            return self.is_stable_member(s);
        }
        let owner = self.u.symbol(s).owner.map(|o| self.u.full_name(o));
        self.has_constant_type(tree) && owner.is_some_and(|o| PURE_OP_OWNERS.contains(&o.as_str()))
    }

    fn has_constant_type(&self, tree: TNodeId) -> bool {
        let tpe = self.tpd.get(tree).tpe;
        tpe.is_some_and(|t| matches!(self.u.types().get(t), Type::Constant(..) | Type::Str(_)))
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
            let shown = self.show_type(widened);
            let msg = format!("Discarded non-Unit value of type {shown}. Add `: Unit` to discard silently.");
            let at = self.tpd.span(tree).point();
            self.diags.warning(at, msg);
        }
    }
}
