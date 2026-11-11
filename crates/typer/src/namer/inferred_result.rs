// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala
//! The result type of a script definition: its type tree's, else its right-hand side's, widened
//! (dotty's `valOrDefDefSig`, `inferredResultType`, `widenRhs`).

use crate::ctx::Ctx;
use crate::typer::deskolemize::deskolemized;
use crate::typer::{Pt, Typer, Widen};
use syntax::ast::{NodeId, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl Typer<'_> {
    /// The type of definition `sym`: its type tree's, else its right-hand side's, widened.
    pub fn result_type(&mut self, sym: SymbolId, tpt: NodeId, rhs: Option<NodeId>, ctx: Ctx) -> TypeId {
        if !self.is_inferred_tpt(tpt) {
            return self.value_type_ahead(tpt, ctx);
        }
        let Some(rhs) = rhs else { return self.u.intern_type(Type::Error) };
        let inherited = self.inherited_result(sym);
        match inherited {
            Some(i) if !self.keeps_rhs_type(sym) => return i,
            Some(_) => {}
            None => self.check_inferred_implicit(sym),
        }
        let (param_type, by_name) = self.default_param_proto(sym);
        let raw_default = param_type.map(|(_, pt)| pt);
        let exact = param_type.is_some_and(|(e, pt)| e == pt);
        let default = raw_default.filter(|&d| !self.mentions_own_type_params(d, sym));
        self.check_context_default(rhs, raw_default.filter(|_| default.is_none()));
        let lambda = matches!(self.ast.tree(rhs), Tree::Function { .. });
        let approx = raw_default.filter(|_| lambda && default.is_none()).and_then(|d| {
            let a = self.own_type_params_approximated(d, sym);
            if a.is_none() {
                self.unsupported(rhs, "function default of a bounded type parameter");
            }
            a
        });
        let pt = inherited.or(default).or(approx).map_or(Pt::Any, Pt::Type);
        let typed = self.typed(rhs, pt, ctx);
        self.typed_ahead(rhs, typed, ctx);
        let mut tpe = self.tpd.tpe(typed);
        let inline_param = self.syms.flags.get(&sym).is_some_and(|fl| fl.is(syntax::ast::flags::INLINE));
        if inherited.is_none()
            && let Some(d) = default.filter(|_| exact && !by_name && !inline_param)
        {
            tpe = self.fully_defined_rhs(tpe, rhs);
            if self.u.is_subtype(tpe, d) {
                return self.default_getter_type(d);
            }
        }
        let mut wildcard = false;
        self.u.map_type(tpe, &mut |u, part| {
            wildcard |= matches!(u.ty(part), Type::Wildcard(_));
            None
        });
        if wildcard && raw_default.is_some() {
            self.unsupported(rhs, "wildcard in the inferred type of a default");
        }
        let widened = self.widen_rhs(sym, tpe);
        let widened = self.simplified_inferred_as(widened, raw_default.is_some());
        if self.keeps_rhs_type(sym) && matches!(self.u.ty(widened), Type::Constant(..) | Type::Str(_)) {
            return self.fully_defined_rhs(widened, rhs);
        }
        let bound = inherited.or(default).or(approx).unwrap_or(tasty::ANY);
        let widened = self.widen_inferred(widened, bound, Widen::Unions).unwrap_or_else(|what| {
            self.unsupported(rhs, what);
            widened
        });
        let defined = self.fully_defined_rhs(widened, rhs);
        let widened = if defined == widened {
            defined
        } else {
            self.simplified_inferred_as(defined, raw_default.is_some())
        };
        deskolemized(self.u, widened, 1).unwrap_or_else(|| {
            self.unsupported(rhs, "skolem in an inferred type");
            widened
        })
    }

    /// `t` with its type variables instantiated (dotty's `fullyDefinedType`); `t` and a refusal
    /// of `rhs` where that fails.
    fn fully_defined_rhs(&mut self, t: TypeId, rhs: NodeId) -> TypeId {
        self.fully_defined_as(t, true).unwrap_or_else(|what| {
            self.unsupported(rhs, what);
            t
        })
    }

    /// The type inferred for definition `sym` from its right-hand side's type `t` (dotty's
    /// `widenRhs`): a constant for a final value, a singleton for an object, else `t` widened.
    fn widen_rhs(&mut self, sym: SymbolId, t: TypeId) -> TypeId {
        let inline_val = self.keeps_rhs_type(sym);
        let ty = *self.u.ty(t);
        match ty {
            Type::Constant(..) | Type::Str(_) if inline_val => t,
            Type::Constant(..) | Type::Str(_) => self.u.widen(t),
            Type::Annotated(seq, annot) if self.is_repeated_annot(annot) => seq,
            Type::Sym(_, s) if self.u.symbol(s).flags.has(tasty::tags::OBJECT) => t,
            Type::Sym(_, s) if !self.u.is_type_symbol(s) && self.u.symbol(s).kind != Kind::Package => {
                let info = self.method_info(t).unwrap_or(t);
                self.widen_rhs(sym, info)
            }
            Type::ByName(r) | Type::Skolem(_, r) => self.widen_rhs(sym, r),
            Type::This(c) => self.widened_this(t, c),
            _ => t,
        }
    }
}
