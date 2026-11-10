// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//   compiler/src/dotty/tools/dotc/typer/Linter.scala
//! dotty's `Linter.warnOnInterestingResultInStatement`: `-Wnonunit-statement`.

mod this_type_results;

use super::Typer;
use super::type_ops::TypeOps;
use crate::tpd::{TNodeId, Tpd, Tree as T};
use syntax::config::Lints;
use syntax::diag::{Message, MessageId};
use tasty::{Kind, SymbolId, Type, TypeId};

/// Decides whether a statement's value is worth a warning under `-Wnonunit-statement`: the
/// result of a call such as `list.map(f)` is, while `()`, `sb.append(x)` returning its receiver,
/// `x += 1`, a constructor call and a call of a Java method are not.
pub struct Linter<'a> {
    tpd: &'a Tpd,
    /// The class `scala.Unit`.
    unit: SymbolId,
}

impl Typer<'_> {
    /// Warns about statement `t` if it has an interesting result under `-Wnonunit-statement`;
    /// whether it did.
    pub fn warn_on_interesting_result(&mut self, t: TNodeId) -> bool {
        if !self.lints.contains(Lints::NONUNIT_STATEMENT) {
            return false;
        }
        let linter = Linter { tpd: &self.tpd, unit: self.defn.unit };
        if !linter.interesting_shape(&mut TypeOps { u: self.u, constraint: &mut self.constraint }, t) {
            return false;
        }
        let at = match *self.tpd.tree(t) {
            T::Block { expr, .. } => expr,
            T::If { thenp, elsep, .. } if matches!(self.tpd.tree(elsep), T::Literal { .. }) => {
                match *self.tpd.tree(thenp) {
                    T::Block { expr, .. } => expr,
                    _ => thenp,
                }
            }
            _ => t,
        };
        let tpe = self.tpd.tpe(at);
        let span = self.tpd.span(t);
        let method_ref = self.term_symbol(tpe).is_some_and(|s| self.u.symbol(s).kind == Kind::Def);
        let Some(shown) = self.found_text(tpe).filter(|_| !method_ref) else {
            self.unsupported_at(span, "an unused value of this type under -Wnonunit-statement");
            return true;
        };
        let text = format!("unused value of type {shown}");
        let id = MessageId { code: 176, kind: "Potential Issue", explained: false };
        self.diags.warning(span.point(), Message { text, id: Some(id) });
        true
    }
}

impl Linter<'_> {
    /// Whether statement `t`, or a branch or the result of a block it is, has an interesting
    /// result.
    fn interesting_shape(&self, ty: &mut TypeOps, t: TNodeId) -> bool {
        match *self.tpd.tree(t) {
            T::If { thenp, elsep, .. } => {
                self.interesting_shape(ty, thenp) || self.interesting_shape(ty, elsep)
            }
            T::Block { expr, .. } => self.interesting_shape(ty, expr),
            T::Match { cases, .. } => self.tpd.items(cases).iter().any(|&c| match *self.tpd.tree(c) {
                T::CaseDef { body, .. } => self.interesting_shape(ty, body),
                _ => false,
            }),
            _ => self.interesting_result(ty, t),
        }
    }

    /// dotty's `checksForInterestingResult`.
    fn interesting_result(&self, ty: &mut TypeOps, t: TNodeId) -> bool {
        let sym = match *self.tpd.tree(t) {
            T::Ident { .. } | T::Select { .. } | T::Apply { .. } | T::TypeApply { .. } => {
                self.tpd.sym(self.tpd.meth_part(t))
            }
            _ => None,
        };
        let Some(sym) = sym else { return false };
        let tpe = self.tpd.tpe(t);
        !uninteresting_symbol(ty, sym)
            && !self.uninteresting_type(ty, tpe)
            && !self.is_this_type_result(ty, t)
            && !self.is_java_application(ty, t)
    }

    fn uninteresting_type(&self, ty: &mut TypeOps, t: TypeId) -> bool {
        let w = ty.u.widen(t);
        if w == tasty::NOTHING || matches!(ty.u.types().get(w), Type::Error) {
            return true;
        }
        let Some(c) = type_class(ty, w) else { return false };
        let b = ty.u.builtins();
        [self.unit, b.nothing, b.null, b.any, b.object].contains(&c)
            || b.any_val == Some(c)
            || ty.u.full_name(c) == "scala.runtime.BoxedUnit"
    }

    /// dotty's `isJavaApplication`: a call of a Java method not of `Object`.
    fn is_java_application(&self, ty: &mut TypeOps, t: TNodeId) -> bool {
        let T::Apply { fun, .. } = *self.tpd.tree(t) else { return false };
        let Some(f) = self.tpd.sym(fun) else { return false };
        let b = ty.u.builtins();
        let owner = ty.u.symbol(f).owner;
        ty.is_java_member(f) && !owner.is_some_and(|o| [b.object, b.any, b.matchable].contains(&o))
    }
}

fn uninteresting_symbol(ty: &mut TypeOps, s: SymbolId) -> bool {
    let b = ty.u.builtins();
    let sym = ty.u.symbol(s);
    let name = sym.name.trim_end_matches('$');
    name == "<init>"
        || sym.kind == Kind::Package
        || name == "package"
        || name.ends_with("$package")
        || [b.any, b.any_ref].contains(&s)
        || b.any_val == Some(s)
        || ty.u.full_name(s) == "scala.runtime.BoxedUnit"
}

/// The class `t` names, through aliases but not abstract types (dotty's `typeSymbol`).
fn type_class(ty: &mut TypeOps, t: TypeId) -> Option<SymbolId> {
    let s = match *ty.u.types().get(t) {
        Type::Sym(_, s) => s,
        Type::TypeRef(..) => ty.u.resolve_ref(t)?,
        Type::Applied(inner, _) | Type::Annotated(inner, _) => return type_class(ty, inner),
        _ => return None,
    };
    match ty.u.symbol(s).kind {
        Kind::Class => Some(s),
        Kind::Type => {
            let info = ty.u.info(s);
            match *ty.u.types().get(info) {
                Type::Alias(a) => type_class(ty, a),
                _ => None,
            }
        }
        _ => None,
    }
}
