// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/TypeOps.scala
//   compiler/src/dotty/tools/dotc/core/Types.scala
//! Selections on an approximated prefix (dotty's `AvoidMap.derivedSelect` and `tryWiden`).

use super::Typer;
use super::approx::{self, Approx};
use super::avoid_map::Avoided;
use tasty::{ANY, Kind, NOTHING, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// `pre.s` with prefix `pre` approximated as `p`: the member's alias, bounds or singleton
    /// type when it has one, a term's widened type at a positive variance, the member of the
    /// upper prefix if it has one, `Nothing .. Any` otherwise.
    pub fn avoid_select(
        &mut self,
        t: TypeId,
        pre: TypeId,
        p: Approx,
        s: SymbolId,
        v: i8,
        a: Avoided,
    ) -> Option<Approx> {
        if p == Approx::Exact(pre) {
            return Some(Approx::Exact(t));
        }
        let singleton = matches!(p, Approx::Exact(q) if self.is_path(q));
        if !singleton && let Some(r) = self.try_widen(t, pre, s, v, a) {
            return r;
        }
        if self.u.symbol(s).kind.is_term() && v > 0 && !singleton {
            return self.avoid_term_info(pre, s, v, a);
        }
        let name = self.u.symbol(s).name.clone();
        if self.type_members(p.hi(), &name).is_empty() {
            return Some(approx::range(self.u, Approx::Exact(NOTHING), Approx::Exact(ANY), v));
        }
        let hi = self.u.intern_type(Type::Sym(Some(p.hi()), s));
        let lo = match p.lo() {
            NOTHING => NOTHING,
            lo => self.u.intern_type(Type::Sym(Some(lo), s)),
        };
        Some(approx::range(self.u, Approx::Exact(lo), Approx::Exact(hi), v))
    }

    /// `pre.s` whose prefix cannot be approximated: what it is equal to or bounded by, or a
    /// term's widened type at a positive variance.
    pub fn avoid_select_on_failed(
        &mut self,
        t: TypeId,
        pre: TypeId,
        s: SymbolId,
        v: i8,
        a: Avoided,
    ) -> Option<Approx> {
        if let Some(r) = self.try_widen(t, pre, s, v, a) {
            return r;
        }
        (self.u.symbol(s).kind.is_term() && v > 0).then(|| self.avoid_term_info(pre, s, v, a))?
    }

    /// The widened type of term `s` selected on `pre`, approximated.
    fn avoid_term_info(&mut self, pre: TypeId, s: SymbolId, v: i8, a: Avoided) -> Option<Approx> {
        let info = self.u.term_type(Some(pre), s);
        let info = self.u.widen(info);
        if matches!(self.u.types().get(info), Type::Bounds(..)) {
            return self.avoid_info(info, v, a);
        }
        self.avoid_at(info, v, a)
    }

    /// `pre.s` replaced by what it is equal to or bounded by (dotty's `tryWiden` and
    /// `expandParam`); `None` if it has no such type, `Some(None)` if avoiding that fails.
    fn try_widen(
        &mut self,
        t: TypeId,
        pre: TypeId,
        s: SymbolId,
        v: i8,
        a: Avoided,
    ) -> Option<Option<Approx>> {
        match self.u.symbol(s).kind {
            Kind::TypeParam => {
                let arg = self.resolve_class_params(t);
                (arg != t).then(|| match *self.u.types().get(arg) {
                    Type::Bounds(..) => self.avoid_info(arg, v, a),
                    _ => self.avoid_at(arg, v, a),
                })
            }
            Kind::Type => {
                let alias = self.u.dealias(t);
                if alias != t {
                    return Some(self.avoid_at(alias, v, a));
                }
                let info = self.u.member_info(pre, s);
                matches!(self.u.types().get(info), Type::Bounds(..)).then(|| self.avoid_info(info, v, a))
            }
            k if k.is_term() => {
                let info = self.u.term_type(Some(pre), s);
                let info = match *self.u.types().get(info) {
                    Type::ByName(r) => r,
                    _ => info,
                };
                self.is_path(info).then(|| self.avoid_at(info, v, a))
            }
            _ => None,
        }
    }

    /// Whether `t` is a singleton type: a path, `this`, a constant or a skolem.
    pub fn is_path(&mut self, t: TypeId) -> bool {
        match *self.u.types().get(t) {
            Type::TermRef(..) | Type::This(_) | Type::Constant(..) | Type::Str(_) | Type::Skolem(..) => true,
            Type::Sym(_, s) => self.u.symbol(s).kind.is_term(),
            _ => false,
        }
    }
}
