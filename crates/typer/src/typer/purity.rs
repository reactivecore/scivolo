// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//   compiler/src/dotty/tools/dotc/typer/Typer.scala
//! How far evaluating a tree has effects (dotty's `exprPurity`, `statPurity`, `refPurity`).

use super::Typer;
use super::java_classes::is_java_member;
use crate::tpd::{TNodeId, Tpd, Tree as T};
use names::nme;
use tasty::{Kind, SymbolId, Type, Universe};

/// dotty's `PurityLevel` bits: evaluating twice gives the same, evaluating has no effect, the
/// tree is a path; the weaker of two levels is their intersection.
const IDEMPOTENT: u8 = 1;
const PURE: u8 = 2 | IDEMPOTENT;
const IDEMPOTENT_PATH: u8 = 4 | IDEMPOTENT;
const PURE_PATH: u8 = 4 | PURE;
const IMPURE: u8 = 0;
/// The methods whose type application is pure.
const PURE_TYPE_APPLIES: [&str; 4] = [
    "scala.Predef$.classOf",
    "scala.quoted.Type$.of",
    "scala.caps.internal$.erasedValue",
    "scala.caps.unsafe$.unsafeErasedValue",
];

impl Typer<'_> {
    /// Tells whether evaluating `tree` has no effect, as for a literal, a `val` or `new C` of a
    /// class whose construction runs no code.
    pub fn is_pure_expr(&mut self, tree: TNodeId) -> bool {
        expr_purity(self, tree) & PURE == PURE
    }

    /// Tells whether `rhs` of an inline value of a constant type is pure once `FirstTransform`
    /// makes it a literal: idempotent if it refers to a final or inline value (dotty's
    /// `constToLiteral`).
    pub fn is_inline_value_rhs(&mut self, rhs: TNodeId) -> bool {
        use tasty::tags::{FINAL, INLINE, MUTABLE};
        let inline_val = matches!(self.tpd.tree(rhs), T::Ident { .. } | T::Select { .. })
            && self.tpd.sym(rhs).is_some_and(|s| {
                let s = self.u.symbol(s);
                s.kind == Kind::Val && (s.flags.has(INLINE) || s.flags.has(FINAL)) && !s.flags.has(MUTABLE)
            });
        if inline_val { self.is_idempotent_expr(rhs) } else { self.is_pure_expr(rhs) }
    }

    /// Tells whether evaluating `tree` twice has the effect of evaluating it once, as for a
    /// `lazy val`.
    pub fn is_idempotent_expr(&mut self, tree: TNodeId) -> bool {
        expr_purity(self, tree) & IDEMPOTENT == IDEMPOTENT
    }

    /// Tells whether `tree` is a path whose evaluation has no effect, as `a.b` of stable values
    /// (dotty's `isPurePath`).
    pub fn is_pure_path(&mut self, tree: TNodeId) -> bool {
        expr_purity(self, tree) == PURE_PATH
    }
}

fn expr_purity(t: &mut Typer, tree: TNodeId) -> u8 {
    match t.tpd.tree(tree).clone() {
        T::Literal { .. } | T::ClassLiteral { .. } | T::This { .. } | T::Super { .. } => PURE_PATH,
        T::New { .. } | T::Closure { .. } => PURE,
        T::NamedArg { arg, .. } => expr_purity(t, arg),
        T::Ident { .. } => ref_purity(t, tree),
        T::Select { qualifier, .. } => ref_purity(t, tree) & expr_purity(t, qualifier),
        T::TypeApply { fun, .. } if is_method_typed(t, tree) => expr_purity(t, fun),
        T::TypeApply { fun, .. } => {
            let name = t.tpd.sym(fun).map(|s| t.u.full_name(s));
            let qualifier = match *t.tpd.tree(fun) {
                T::Select { qualifier, .. } => Some(qualifier),
                _ => None,
            };
            match (name.as_deref(), qualifier) {
                (Some("scala.Any.$asInstanceOf$"), Some(q)) => expr_purity(t, q) & PURE,
                (Some("scala.compiletime.package$package$.erasedValue"), _)
                    if has_constant_type(&t.tpd, t.u, tree) =>
                {
                    PURE
                }
                (Some(n), _) if PURE_TYPE_APPLIES.contains(&n) => PURE,
                _ => IMPURE,
            }
        }
        T::Apply { fun, args } => {
            let args = t.tpd.items(args).to_vec();
            let factor = args.into_iter().fold(expr_purity(t, fun), |p, a| p & expr_purity(t, a));
            let callee = callee(&t.tpd, fun);
            if is_method_typed(t, tree) || is_pure_apply(t, tree, callee) {
                factor & PURE
            } else if callee.is_some_and(|s| t.is_stable_member(s)) {
                factor & IDEMPOTENT
            } else {
                IMPURE
            }
        }
        T::Typed { expr, .. } => expr_purity(t, expr),
        T::Block { stats, expr: last } | T::Inlined { bindings: stats, expansion: last, .. } => {
            let stats = t.tpd.items(stats).to_vec();
            stats.into_iter().fold(expr_purity(t, last), |p, s| p & stat_purity(t, s))
        }
        _ => IMPURE,
    }
}

fn stat_purity(t: &mut Typer, s: TNodeId) -> u8 {
    match *t.tpd.tree(s) {
        T::DefDef { .. } | T::TypeDef { .. } | T::Import { .. } => PURE,
        T::ValDef { rhs, .. } => {
            let mutable = t.tpd.sym(s).is_some_and(|v| t.u.symbol(v).flags.has(tasty::tags::MUTABLE));
            match rhs {
                _ if mutable => IMPURE,
                Some(r) => expr_purity(t, r) & PURE,
                None => PURE,
            }
        }
        _ => IMPURE,
    }
}

/// The purity of reference `tree`: a path if it is to a method not applied yet, of a constant
/// type, or to a stable value; an idempotent path for a package, a lazy value or an object that
/// runs code when created.
fn ref_purity(t: &mut Typer, tree: TNodeId) -> u8 {
    if is_method_typed(t, tree) || has_constant_type(&t.tpd, t.u, tree) {
        return PURE_PATH;
    }
    let Some(s) = t.tpd.sym(tree) else { return IMPURE };
    if t.u.symbol(s).kind == Kind::Package {
        return IDEMPOTENT_PATH;
    }
    if !t.is_stable_member(s) {
        return IMPURE;
    }
    if t.u.is_module(s) {
        let no_inits = t.u.module_class(s).is_some_and(|c| {
            let bases = t.u.linearization(c);
            t.u.is_assured_no_inits(c) || bases.iter().all(|&b| t.is_no_inits(b))
        });
        return if no_inits { PURE_PATH } else { IDEMPOTENT_PATH };
    }
    if t.u.symbol(s).flags.has(tasty::tags::LAZY) { IDEMPOTENT_PATH } else { PURE_PATH }
}

/// Whether `tree` has a method type or refers to a method with parameters, unapplied.
fn is_method_typed(t: &mut Typer, tree: TNodeId) -> bool {
    let tpe = t.tpd.tpe(tree);
    let info = t.method_info(tpe).unwrap_or(tpe);
    matches!(t.u.ty(info), Type::Method { .. } | Type::Poly { .. })
}

/// dotty's `isPureApply`: a constant operation of a primitive class, `String` or a pure member
/// of `Any`, a stable constructor of a class outside the JDK, or the synthetic `apply` of a case
/// class that initializes nothing.
fn is_pure_apply(t: &mut Typer, tree: TNodeId, callee: Option<SymbolId>) -> bool {
    let Some(s) = callee else { return false };
    if t.u.symbol(s).name == nme::CONSTRUCTOR {
        return t.is_stable_member(s) && !is_java_member(t.u, s);
    }
    if is_pure_case_apply(t, s) {
        return true;
    }
    has_constant_type(&t.tpd, t.u, tree) && t.u.is_known_pure_op(s)
}

/// dotty's `isPureSyntheticCaseApply`.
fn is_pure_case_apply(t: &mut Typer, s: SymbolId) -> bool {
    let sym = t.u.symbol(s);
    if sym.name != nme::APPLY || !sym.flags.has(tasty::tags::SYNTHETIC) {
        return false;
    }
    let Some(module) = sym.owner else { return false };
    let Some(name) = tasty::module_name(&t.u.symbol(module).name).map(str::to_owned) else {
        return false;
    };
    let Some(owner) = t.u.symbol(module).owner else { return false };
    let classes = t.u.decls_named(owner, &name);
    classes.into_iter().any(|c| {
        let flags = t.u.symbol(c).flags;
        t.u.symbol(c).kind == Kind::Class
            && flags.has(tasty::tags::CASE)
            && !flags.has(tasty::tags::TRAIT)
            && (t.is_no_inits(c) || t.u.is_assured_no_inits(c))
    })
}

fn has_constant_type(tpd: &Tpd, u: &Universe, tree: TNodeId) -> bool {
    let tpe = tpd.get(tree).tpe;
    tpe.is_some_and(|x| matches!(u.ty(x), Type::Constant(..) | Type::Str(_)))
}

/// The method a function tree refers to.
fn callee(tpd: &Tpd, fun: TNodeId) -> Option<SymbolId> {
    match *tpd.tree(fun) {
        T::Ident { .. } | T::Select { .. } => tpd.sym(fun),
        T::TypeApply { fun, .. } => callee(tpd, fun),
        _ => None,
    }
}
