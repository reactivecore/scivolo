//! The `CanEqual` instances the compiler assumes (dotty's `synthesizedCanEqual`): for predefined
//! classes that compare, and `CanEqual.canEqualAny` where neither side has a reflexive instance.

use super::super::Typer;
use super::search::is_misfit;
use crate::ctx::{Ctx, Mode};
use crate::tpd::TNodeId;
use syntax::ast::Span;
use syntax::config::Features;
use tasty::{Kind, SymbolId, Type, TypeId};

const NUMERIC: [&str; 7] = ["Byte", "Short", "Char", "Int", "Long", "Float", "Double"];
const BOXED: [(&str, &str); 9] = [
    ("scala.Byte", "java.lang.Byte"),
    ("scala.Short", "java.lang.Short"),
    ("scala.Char", "java.lang.Character"),
    ("scala.Int", "java.lang.Integer"),
    ("scala.Long", "java.lang.Long"),
    ("scala.Float", "java.lang.Float"),
    ("scala.Double", "java.lang.Double"),
    ("scala.Boolean", "java.lang.Boolean"),
    ("scala.Unit", "scala.runtime.BoxedUnit"),
];

impl Typer<'_> {
    /// `CanEqual[l, r]`.
    pub fn can_equal_type(&mut self, l: TypeId, r: TypeId) -> Option<TypeId> {
        let class = self.resolve_full_name("scala.CanEqual", true)?;
        Some(crate::defn::applied(self.u, class, vec![l, r]))
    }

    /// `CanEqual.canEqualAny[l, r]` at `at` if `CanEqual[l, r]` may be assumed in `ctx`.
    pub fn synthesized_can_equal(
        &mut self,
        l: TypeId,
        r: TypeId,
        at: Span,
        ctx: Ctx,
        depth: u32,
    ) -> Result<Option<TNodeId>, &'static str> {
        let (l, r) = (self.fully_defined(l)?, self.fully_defined(r)?);
        let strict = self.is_strict_equality(ctx);
        let assumed = self.can_compare_predefined(l, r)
            || !strict
                && (self.assumed_can_equal(l, r)
                    || !self.has_eq(l, at, ctx, depth)? && !self.has_eq(r, at, ctx, depth)?);
        if !assumed {
            return Ok(None);
        }
        let module = self.resolve_full_name("scala.CanEqual", false).ok_or("CanEqual")?;
        let module_class = self.u.module_class(module).ok_or("CanEqual")?;
        let any = self.u.decl(module_class, "canEqualAny").unwrap_or_default();
        let any =
            any.into_iter().find(|&d| self.u.symbol(d).kind == Kind::Def).ok_or("CanEqual.canEqualAny")?;
        let module_tpe = crate::defn::type_ref(self.u, module);
        let qualifier = self.build().ident(module, module_tpe, at);
        let select = self.build().select(qualifier, any, at);
        let targs = [self.type_tree(l, at, false), self.type_tree(r, at, false)];
        let tpe = self.instantiated(select, &[l, r])?;
        Ok(Some(self.build().type_apply(select, &targs, tpe, at)))
    }

    /// Whether `ctx` compares under `strictEquality`, by the language feature or while searching
    /// for a reflexive instance (dotty's `Mode.StrictEquality`).
    pub fn is_strict_equality(&mut self, ctx: Ctx) -> bool {
        ctx.mode.is(Mode::STRICT_EQUALITY)
            || self.feature_enabled(Features::STRICT_EQUALITY, "strictEquality", ctx)
    }

    /// Whether a `CanEqual[t, t]` instance exists under `strictEquality`.
    fn has_eq(&mut self, t: TypeId, at: Span, ctx: Ctx, depth: u32) -> Result<bool, &'static str> {
        let pt = self.can_equal_type(t, t).ok_or("CanEqual")?;
        let (before, unsupported) = (self.constraint.attempt(), self.unsupported.clone());
        let result = self.implicit_arg(pt, at, ctx.with_mode(Mode::STRICT_EQUALITY), depth + 1);
        let clean = self.unsupported == unsupported;
        self.constraint.abandon(before);
        self.unsupported = unsupported;
        match result {
            _ if !clean => Err("reflexive CanEqual search outside the subset"),
            Ok(_) => Ok(true),
            Err(what) if is_misfit(what) => Ok(false),
            Err(what) => Err(what),
        }
    }

    /// Whether `CanEqual` is assumed for the classes of `l` and `r`: the same or two numeric
    /// value classes, a value class and its box, `Null` and a reference class, or `Nothing`.
    fn can_compare_predefined(&mut self, l: TypeId, r: TypeId) -> bool {
        let (ls, rs) = (self.class_symbols(l), self.class_symbols(r));
        ls.iter().any(|&a| rs.iter().any(|&b| self.can_compare_classes(a, b)))
    }

    fn can_compare_classes(&mut self, a: SymbolId, b: SymbolId) -> bool {
        let (na, nb) = (self.u.full_name(a), self.u.full_name(b));
        let primitive = |n: &str| BOXED.iter().any(|(p, _)| *p == n);
        let numeric = |n: &str| NUMERIC.iter().any(|x| n.strip_prefix("scala.") == Some(*x));
        let builtins = self.u.builtins();
        let with_boxed = |s: &mut Self, prim: &str, other: SymbolId, other_name: &str| {
            other == builtins.nothing
                || BOXED.iter().any(|&(p, boxed)| p == prim && boxed == other_name)
                || numeric(prim)
                    && s.resolve_full_name("java.lang.Number", true)
                        .is_some_and(|n| s.u.derives_from(other, n))
        };
        if primitive(&na) {
            if primitive(&nb) {
                na == nb || numeric(&na) && numeric(&nb)
            } else {
                with_boxed(self, &na, b, &nb)
            }
        } else if primitive(&nb) {
            with_boxed(self, &nb, a, &na)
        } else if a == builtins.null {
            a == b || self.u.derives_from(b, self.defn.object)
        } else if b == builtins.null {
            self.u.derives_from(a, self.defn.object)
        } else {
            a == builtins.nothing || b == builtins.nothing
        }
    }

    /// The classes `t` refers to: those of either side of an intersection, those common to both
    /// sides of a union (dotty's `classSymbols`).
    fn class_symbols(&mut self, t: TypeId) -> Vec<SymbolId> {
        let t = self.u.widen(t);
        let t = self.u.dealias(t);
        match *self.u.types().get(t) {
            Type::And(a, b) => [self.class_symbols(a), self.class_symbols(b)].concat(),
            Type::Or(a, b, _) => {
                let right = self.class_symbols(b);
                self.class_symbols(a).into_iter().filter(|c| right.contains(c)).collect()
            }
            _ => self.u.class_symbol(t).into_iter().collect(),
        }
    }
}
