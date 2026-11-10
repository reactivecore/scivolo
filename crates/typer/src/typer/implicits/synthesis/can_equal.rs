// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Implicits.scala
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! The `CanEqual` instances the compiler assumes (dotty's `synthesizedCanEqual`): for predefined
//! classes that compare, and `CanEqual.canEqualAny` where neither side has a reflexive instance.

use super::Synthesizer;
use crate::ctx::Mode;
use crate::defn::CAN_EQUAL;
use crate::tpd::TNodeId;
use crate::typer::Typer;
use tasty::{Kind, SymbolId, Type, TypeId, Universe};

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

impl Synthesizer {
    /// Synthesizes `CanEqual.canEqualAny[l, r]` for a comparison of `l` with `r` that needs no
    /// given instance: of two numbers, a value class and its box, `null` and a reference, or,
    /// without `strictEquality`, of types that always compare or of which neither has a
    /// `CanEqual` with itself.
    pub fn can_equal(self, t: &mut Typer<'_>, l: TypeId, r: TypeId) -> Result<Option<TNodeId>, &'static str> {
        let at = self.at;
        let (l, r) = (t.fully_defined_as(l, true)?, t.fully_defined_as(r, true)?);
        let strict = t.is_strict_equality(self.ctx);
        let assumed = can_compare_predefined(t, l, r)
            || !strict && (t.assumed_can_equal(l, r) || !self.has_eq(t, l)? && !self.has_eq(t, r)?);
        if !assumed {
            return Ok(None);
        }
        let module = t.resolve_full_name(CAN_EQUAL, false).ok_or("CanEqual")?;
        let module_class = t.u.module_class(module).ok_or("CanEqual")?;
        let any = t.u.decl(module_class, "canEqualAny").unwrap_or_default();
        let any = any.into_iter().find(|&d| t.u.symbol(d).kind == Kind::Def).ok_or("CanEqual.canEqualAny")?;
        let select = t.member_ref(any, at);
        let targs = [t.type_tree(l, at, false), t.type_tree(r, at, false)];
        let tpe = t.instantiated(select, &[l, r])?;
        Ok(Some(t.build().type_apply(select, &targs, tpe, at)))
    }

    /// Whether a `CanEqual[tpe, tpe]` instance exists under `strictEquality`.
    fn has_eq(self, t: &mut Typer<'_>, tpe: TypeId) -> Result<bool, &'static str> {
        let pt = t.can_equal_type(tpe, tpe).ok_or("CanEqual")?;
        let ctx = self.ctx.with_mode(Mode::STRICT_EQUALITY.with(Mode::EXISTENCE));
        t.implicit_exists(pt, self.at, ctx, self.depth + 1)
    }
}

/// Whether `CanEqual` is assumed for the classes of `l` and `r`: the same or two numeric value
/// classes, a value class and its box, `Null` and a reference class, or `Nothing`.
fn can_compare_predefined(t: &mut Typer<'_>, l: TypeId, r: TypeId) -> bool {
    let (ls, rs) = (class_symbols(t.u, l), class_symbols(t.u, r));
    ls.iter().any(|&a| rs.iter().any(|&b| can_compare_classes(t, a, b)))
}

fn can_compare_classes(t: &mut Typer<'_>, a: SymbolId, b: SymbolId) -> bool {
    let (na, nb) = (t.u.full_name(a), t.u.full_name(b));
    let primitive = |n: &str| BOXED.iter().any(|(p, _)| *p == n);
    let numeric = |n: &str| NUMERIC.iter().any(|x| n.strip_prefix("scala.") == Some(*x));
    let builtins = t.u.builtins();
    let with_boxed = |t: &mut Typer<'_>, prim: &str, other: SymbolId, other_name: &str| {
        other == builtins.nothing
            || BOXED.iter().any(|&(p, boxed)| p == prim && boxed == other_name)
            || numeric(prim)
                && t.resolve_full_name("java.lang.Number", true).is_some_and(|n| t.u.derives_from(other, n))
    };
    if primitive(&na) {
        if primitive(&nb) { na == nb || numeric(&na) && numeric(&nb) } else { with_boxed(t, &na, b, &nb) }
    } else if primitive(&nb) {
        with_boxed(t, &nb, a, &na)
    } else if a == builtins.null {
        a == b || t.u.derives_from(b, t.defn.object)
    } else if b == builtins.null {
        t.u.derives_from(a, t.defn.object)
    } else {
        a == builtins.nothing || b == builtins.nothing
    }
}

/// The classes `t` refers to: those of either side of an intersection, those common to both
/// sides of a union (dotty's `classSymbols`).
fn class_symbols(u: &mut Universe, t: TypeId) -> Vec<SymbolId> {
    let t = u.widen(t);
    let t = u.dealias(t);
    match *u.types().get(t) {
        Type::And(a, b) => [class_symbols(u, a), class_symbols(u, b)].concat(),
        Type::Or(a, b, _) => {
            let right = class_symbols(u, b);
            class_symbols(u, a).into_iter().filter(|c| right.contains(c)).collect()
        }
        _ => u.class_symbol(t).into_iter().collect(),
    }
}
