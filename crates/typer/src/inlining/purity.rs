//! How pure typed trees are, as the inliner asks (dotty's `exprPurity`, `refPurity` and
//! `Inliner.isElideableExpr`).

use super::Inlining;
use crate::tpd::{TNodeId, Tree as T};
use tasty::tags::{INLINE, LAZY, MUTABLE, STABLE};
use tasty::{Kind, SymbolId, Type};

pub const IMPURE: u8 = 0;
pub const IDEMPOTENT: u8 = 1;
pub const PURE: u8 = 3;
const PATH: u8 = 4;
pub const IDEMPOTENT_PATH: u8 = IDEMPOTENT | PATH;
pub const PURE_PATH: u8 = PURE | PATH;

/// The classes whose operations on constants dotty folds.
pub const PRIMITIVE_OWNERS: [&str; 9] = [
    "scala.Int",
    "scala.Long",
    "scala.Float",
    "scala.Double",
    "scala.Char",
    "scala.Short",
    "scala.Byte",
    "scala.Boolean",
    "java.lang.String",
];

impl Inlining<'_> {
    pub fn purity(&mut self, t: TNodeId) -> u8 {
        match self.tpd.tree(t).clone() {
            T::This { .. } | T::Super { .. } | T::Literal { .. } | T::ClassLiteral { .. } => PURE_PATH,
            T::Ident { .. } => self.ref_purity(t),
            T::Select { qualifier, .. } => self.ref_purity(t).min(self.purity(qualifier)),
            T::New { .. } | T::Closure { .. } => PURE,
            T::TypeApply { fun, .. } => {
                let applied = self.tpd.get(t).tpe.is_none_or(|x| {
                    let w = self.u.widen(x);
                    !matches!(self.u.types().get(w), Type::Method { .. } | Type::Poly { .. })
                });
                let callee = self.tpd.sym(fun).map(|s| self.u.full_name(s));
                match callee.as_deref() {
                    _ if !applied => self.purity(fun),
                    Some("scala.Any.asInstanceOf") => match *self.tpd.tree(fun) {
                        T::Select { qualifier, .. } => self.purity(qualifier).min(PURE),
                        _ => IMPURE,
                    },
                    Some("scala.Predef.classOf") => PURE,
                    _ => IMPURE,
                }
            }
            T::Apply { fun, args } => {
                let args = self.tpd.items(args).to_vec();
                let all = args.into_iter().fold(self.purity(fun), |p, a| p.min(self.purity(a)));
                let callee = self.method_part(fun).and_then(|f| self.tpd.sym(f));
                if self.is_pure_apply(t, callee) {
                    all.min(PURE)
                } else if callee.is_some_and(|s| self.is_stable_member(s)) {
                    all.min(IDEMPOTENT)
                } else {
                    IMPURE
                }
            }
            T::Typed { expr, .. } | T::NamedArg { arg: expr, .. } => self.purity(expr),
            T::Block { stats: defs, expr } | T::Inlined { bindings: defs, expansion: expr, .. } => {
                let defs = self.tpd.items(defs).to_vec();
                defs.into_iter().fold(self.purity(expr), |p, s| p.min(self.stat_purity(s)))
            }
            _ => IMPURE,
        }
    }

    fn stat_purity(&mut self, s: TNodeId) -> u8 {
        match *self.tpd.tree(s) {
            T::DefDef { .. } | T::TypeDef { .. } | T::Import { .. } => PURE,
            T::ValDef { rhs, .. } => {
                let mutable = self.tpd.sym(s).is_some_and(|v| self.u.symbol(v).flags.has(MUTABLE));
                match rhs {
                    _ if mutable => IMPURE,
                    Some(r) => self.purity(r).min(PURE),
                    None => PURE,
                }
            }
            _ => IMPURE,
        }
    }

    fn ref_purity(&mut self, t: TNodeId) -> u8 {
        let Some(tpe) = self.tpd.get(t).tpe else { return IMPURE };
        let widened = self.u.widen(tpe);
        if matches!(self.u.types().get(widened), Type::Method { .. } | Type::Poly { .. })
            || matches!(self.u.types().get(tpe), Type::Constant(..) | Type::Str(_))
        {
            return PURE_PATH;
        }
        let Some(s) = self.tpd.sym(t) else { return IMPURE };
        if !self.is_stable_member(s) {
            return IMPURE;
        }
        if self.u.is_module(s) {
            let no_inits = self.u.module_class(s).is_some_and(|c| self.u.no_inits(c));
            return if no_inits { PURE_PATH } else { IDEMPOTENT_PATH };
        }
        if self.u.symbol(s).flags.has(LAZY) { IDEMPOTENT_PATH } else { PURE_PATH }
    }

    /// Whether term `s` denotes the same value each time (dotty's `isStableMember`).
    pub fn is_stable_member(&mut self, s: SymbolId) -> bool {
        let sym = self.u.symbol(s);
        match sym.kind {
            Kind::Param if sym.flags.has(INLINE) => false,
            Kind::Param | Kind::Val if sym.flags.has(MUTABLE) => false,
            Kind::Param | Kind::Val => {
                let info = self.u.info(s);
                !matches!(self.u.types().get(info), Type::ByName(_))
            }
            Kind::Def => sym.flags.has(STABLE),
            _ => true,
        }
    }

    /// Whether `t`, a call of `callee`, is a constant operation of a primitive class or a
    /// constructor call (dotty's `isPureApply`, without synthetic case applies).
    fn is_pure_apply(&mut self, t: TNodeId, callee: Option<SymbolId>) -> bool {
        let Some(s) = callee else { return false };
        if self.u.symbol(s).name == "<init>" {
            return self.is_stable_member(s);
        }
        let Some(tpe) = self.tpd.get(t).tpe else { return false };
        let constant = matches!(self.u.types().get(tpe), Type::Constant(..) | Type::Str(_));
        let owner = self.u.symbol(s).owner.map(|o| self.u.full_name(o));
        constant && owner.is_some_and(|o| PRIMITIVE_OWNERS.contains(&o.as_str()))
    }

    pub fn is_pure_expr(&mut self, t: TNodeId) -> bool {
        self.purity(t) >= PURE
    }

    pub fn is_idempotent_expr(&mut self, t: TNodeId) -> bool {
        self.purity(t) >= IDEMPOTENT
    }

    /// Whether `t` is an idempotent path, or idempotent of a constant type (dotty's
    /// `isIdempotentPath`).
    pub fn is_idempotent_path(&mut self, t: TNodeId) -> bool {
        let constant = self
            .tpd
            .get(t)
            .tpe
            .is_some_and(|x| matches!(self.u.types().get(x), Type::Constant(..) | Type::Str(_)));
        self.purity(t) >= if constant { IDEMPOTENT } else { IDEMPOTENT_PATH }
    }

    /// Whether `t` can be dropped or duplicated by the inliner (dotty's `isElideableExpr`,
    /// without synthetic case applies).
    pub fn is_elideable(&mut self, t: TNodeId) -> bool {
        match self.tpd.tree(t).clone() {
            T::This { .. } | T::Super { .. } | T::Literal { .. } | T::New { .. } | T::Closure { .. } => true,
            T::Ident { .. } => {
                let inline_param = self.tpd.sym(t).is_some_and(|s| {
                    let s = self.u.symbol(s);
                    s.kind == Kind::Param && s.flags.has(INLINE)
                });
                self.ref_purity(t) >= PURE_PATH || inline_param
            }
            T::Select { qualifier, .. } => self.ref_purity(t) >= PURE_PATH && self.is_elideable(qualifier),
            T::TypeApply { fun, .. } => self.is_elideable(fun),
            T::Apply { fun, args } => {
                let callee = self.method_part(fun).and_then(|f| self.tpd.sym(f));
                let args = self.tpd.items(args).to_vec();
                self.is_pure_apply(t, callee)
                    && self.is_elideable(fun)
                    && args.into_iter().all(|a| self.is_elideable(a))
            }
            T::Typed { expr, .. } | T::NamedArg { arg: expr, .. } => self.is_elideable(expr),
            T::Block { stats: defs, expr } | T::Inlined { bindings: defs, expansion: expr, .. } => {
                let defs = self.tpd.items(defs).to_vec();
                self.is_elideable(expr) && defs.into_iter().all(|d| self.is_stat_elideable(d))
            }
            _ => false,
        }
    }

    fn is_stat_elideable(&mut self, s: TNodeId) -> bool {
        match *self.tpd.tree(s) {
            T::TypeDef { .. } | T::Import { .. } | T::DefDef { .. } => true,
            T::ValDef { rhs, .. } => {
                let mutable = self.tpd.sym(s).is_some_and(|v| self.u.symbol(v).flags.has(MUTABLE));
                !mutable && rhs.is_none_or(|r| self.is_elideable(r))
            }
            _ => false,
        }
    }
}
