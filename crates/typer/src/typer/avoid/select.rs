// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Selections on an approximated prefix (dotty's `AvoidMap.derivedSelect` and `tryWiden`).

use super::{Avoid, is_path};
use crate::typer::Typer;
use crate::typer::approx::{self, Approx};
use tasty::{Kind, NOTHING, SymbolId, Type, TypeId};

impl Avoid<'_> {
    /// Approximates a selection such as `x.T` or `x.y.type` whose prefix `x` had to be
    /// approximated (to `p`): by what the member stands for when that is known, as the alias of
    /// `type T = Int`, by the type of a value member where a supertype will do, by the same member
    /// of the approximated prefix if it has one, and by any type at all otherwise.
    pub fn select(
        self,
        t: &mut Typer,
        tpe: TypeId,
        pre: TypeId,
        p: Approx,
        s: SymbolId,
        v: i8,
    ) -> Option<Approx> {
        if p == Approx::Exact(pre) {
            return Some(Approx::Exact(tpe));
        }
        let singleton = matches!(p, Approx::Exact(q) if is_path(t.u, q));
        if !singleton && let Some(r) = self.try_widen(t, tpe, pre, s, v) {
            return r;
        }
        if t.u.symbol(s).kind.is_term() && v > 0 && !singleton {
            return self.term_info(t, pre, s, v);
        }
        let name = t.u.symbol(s).name.clone();
        if t.type_members(p.hi(), &name).is_empty() {
            return Some(approx::empty(t.u, v));
        }
        let hi = t.u.intern_type(Type::Sym(Some(p.hi()), s));
        let lo = match p.lo() {
            NOTHING => NOTHING,
            lo => t.u.intern_type(Type::Sym(Some(lo), s)),
        };
        Some(approx::range(t.u, Approx::Exact(lo), Approx::Exact(hi), v))
    }

    /// Approximates a selection such as `x.T` whose prefix `x` cannot be approximated: by what the
    /// member stands for when that is known, or by the type of a value member where a supertype
    /// will do; gives up otherwise.
    pub fn select_on_failed(
        self,
        t: &mut Typer,
        tpe: TypeId,
        pre: TypeId,
        s: SymbolId,
        v: i8,
    ) -> Option<Approx> {
        if let Some(r) = self.try_widen(t, tpe, pre, s, v) {
            return r;
        }
        (t.u.symbol(s).kind.is_term() && v > 0).then(|| self.term_info(t, pre, s, v))?
    }

    /// The widened type of term `s` selected on `pre`, approximated.
    fn term_info(self, t: &mut Typer, pre: TypeId, s: SymbolId, v: i8) -> Option<Approx> {
        let info = t.u.term_type(Some(pre), s);
        let info = t.u.widen(info);
        let a = self.elsewhere();
        if t.u.bounds(info).is_some() {
            return a.info(t, info, v);
        }
        a.at(t, info, v)
    }

    /// `tpe`, `pre.s`, replaced by what it is equal to or bounded by (dotty's `tryWiden` and
    /// `expandParam`); `None` if it has no such type, `Some(None)` if avoiding that fails, also
    /// when the bounds of `s` are reached again while expanding them.
    fn try_widen(
        self,
        t: &mut Typer,
        tpe: TypeId,
        pre: TypeId,
        s: SymbolId,
        v: i8,
    ) -> Option<Option<Approx>> {
        let expanding = [self.expanding, &[s]].concat();
        let cyclic = self.expanding.contains(&s);
        let inner = Avoid { expanding: &expanding, own: Some(s), ..self };
        let a = self.elsewhere();
        match t.u.symbol(s).kind {
            Kind::TypeParam => {
                let arg =
                    crate::typer::seen_from::class_param_arg(t.u, tpe).map(|x| t.resolve_class_params(x));
                arg.map(|arg| match *t.u.ty(arg) {
                    Type::Bounds(..) if cyclic => None,
                    Type::Bounds(..) => inner.elsewhere().info(t, arg, v),
                    _ => a.at(t, arg, v),
                })
            }
            Kind::Type => {
                let alias = t.u.dealias(tpe);
                if alias != tpe {
                    return Some(a.at(t, alias, v));
                }
                let info = t.u.member_info(pre, s);
                let bounds = t.u.bounds(info).is_some();
                bounds.then(|| if cyclic { None } else { inner.info(t, info, v) })
            }
            k if k.is_term() => {
                let info = t.u.term_type(Some(pre), s);
                let info = t.u.by_name(info).unwrap_or(info);
                is_path(t.u, info).then(|| a.at(t, info, v))
            }
            _ => None,
        }
    }
}
