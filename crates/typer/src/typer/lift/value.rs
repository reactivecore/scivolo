// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/EtaExpansion.scala
//! One expression lifted into a local value (dotty's `Lifter.lift`).

use super::super::Typer;
use super::super::type_ops::TypeOps;
use super::Lifter;
use crate::tpd::TNodeId;
use syntax::ast::Span;
use syntax::ast::flags as f;
use tasty::{Kind, Type, TypeId};

impl Lifter {
    /// Gives `expr`, or a reference to `val prefix$N = expr` added to the definitions if it needs
    /// lifting; `$N$` for an empty prefix.
    pub fn lift(&mut self, t: &mut Typer, expr: TNodeId, prefix: &str) -> TNodeId {
        if self.no_lift(t, expr) {
            return expr;
        }
        let name = match prefix {
            "" => t.fresh.unique(t.names),
            p => t.fresh.next(t.names, &format!("{p}$")),
        };
        let text = t.text(name).to_owned();
        let tpe = t.tpd.tpe(expr);
        let tpe = if is_stable_type(&mut t.types(), tpe) { tpe } else { t.u.widen(tpe) };
        let owner = self.ctx.owner;
        let sym = t.syms.define(t.u, &text, Kind::Val, owner, f::SYNTHETIC);
        t.defined_unique(name, sym, t.tpd.span(expr));
        t.u.set_info(sym, tpe);
        t.syms.closure_syms.push(sym);
        t.change_owners(expr, owner, sym);
        let span = t.tpd.span(expr);
        let tpt = t.type_tree(tpe, span.start_pos(), true);
        let def = t.build().val_def(sym, tpt, Some(expr), span);
        self.defs.push(def);
        let reference = t.u.intern_type(Type::Sym(None, sym));
        t.build().ident(sym, reference, Span::at(span.point()))
    }

    /// Whether `expr` stays in place (dotty's `noLift`).
    fn no_lift(&self, t: &mut Typer, expr: TNodeId) -> bool {
        if self.complex { t.is_pure_path(expr) } else { t.is_pure_expr(expr) }
    }
}

/// Whether `tpe` is a stable type (dotty's `isStable`): a reference to a stable member through a
/// stable prefix, a `this`, a package, a constant.
fn is_stable_type(ops: &mut TypeOps, tpe: TypeId) -> bool {
    match *ops.u.types().get(tpe) {
        Type::This(_) | Type::Package(_) | Type::Constant(..) | Type::Str(_) => true,
        Type::Sym(prefix, s) => {
            let kind = ops.u.symbol(s).kind;
            kind == Kind::Package
                || !ops.u.is_type_symbol(s)
                    && ops.is_stable_member(s)
                    && prefix.is_none_or(|p| is_stable_type(ops, p))
        }
        _ => false,
    }
}
