// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//   compiler/src/dotty/tools/dotc/transform/InterceptedMethods.scala
//! Universal equality and hashes of references (dotty's `genEqEqPrimitive`, `InterceptedMethods`'s
//! `poundPoundValue`).

use super::body::Body;
use super::cond::{Test, jump_zero};
use super::{Gen, unsupported};
use classfile::{Invoke, Label};
use lower::Result;
use syntax::ast::constant::Constant;
use tasty::tags::FINAL;
use tasty::{Erased, SymbolId, nme};
use typer::tpd::{TNodeId, Tree};

const OBJECTS: (&str, &str) = ("java/lang/Object", "(Ljava/lang/Object;Ljava/lang/Object;)Z");

impl Gen<'_, '_> {
    /// Jumps to `success` if `left == right` holds for references, else to `failure`; falls
    /// through to `fall`.
    pub fn eq_eq(
        &mut self,
        b: &mut Body,
        (left, right): (TNodeId, TNodeId),
        labels: [Label; 3],
    ) -> Result<()> {
        let [success, failure, fall] = labels;
        self.l.equality()?;
        if self.must_use_any_comparator(left, right) {
            self.expr(b, left)?;
            self.expr(b, right)?;
            b.code.invoke(Invoke::Static, b.pool, "scala/runtime/BoxesRunTime", nme::EQUALS, OBJECTS.1);
        } else if is_null(self, left) || is_null(self, right) {
            self.expr(b, if is_null(self, left) { right } else { left })?;
            jump_zero(b, true, Test::Eq, success, failure, fall);
            return Ok(());
        } else if self.is_non_null(left) {
            self.expr(b, left)?;
            self.expr(b, right)?;
            b.code.invoke(Invoke::Virtual, b.pool, OBJECTS.0, nme::EQUALS, "(Ljava/lang/Object;)Z");
        } else {
            self.expr(b, left)?;
            self.expr(b, right)?;
            b.code.invoke(Invoke::Static, b.pool, "java/util/Objects", nme::EQUALS, OBJECTS.1);
        }
        jump_zero(b, false, Test::Ne, success, failure, fall);
        Ok(())
    }

    /// `##` call `t`: `0` of `null`, `Statics.longHash`, `doubleHash` or `floatHash` of a boxed
    /// `Long`, `Double` or `Float`, else `Statics.anyHash`; `None` for another call.
    pub fn hash(&mut self, b: &mut Body, t: TNodeId) -> Result<Option<i32>> {
        let Tree::Apply { fun, args } = *self.l.tpd.tree(t) else { return Ok(None) };
        let Some(m) = self.l.tpd.sym(fun).filter(|&m| self.l.u.symbol(m).name == nme::HASHHASH) else {
            return Ok(None);
        };
        let owner = self.l.u.symbol(m).owner.map(|o| self.l.u.binary_name(o));
        let (Tree::Select { qualifier, .. }, true, true) = (
            self.l.tpd.tree(fun).clone(),
            self.l.tpd.items(args).is_empty(),
            matches!(owner.as_deref(), Some("scala.Any" | "java.lang.Object")),
        ) else {
            return unsupported(self.l.tpd.span(t), "this `##` on the JVM");
        };
        let statics = "scala/runtime/Statics";
        match lower::erased_type(self.l, qualifier) {
            Erased::Null => b.code.int(b.pool, 0),
            Erased::Prim(_) | Erased::Unit => {
                return unsupported(self.l.tpd.span(t), "`##` of a primitive value");
            }
            _ => match self.boxed_primitive(qualifier) {
                Some((letter, value)) => {
                    let (name, d) = match letter {
                        'j' => ("longHash", "(J)I"),
                        'd' => ("doubleHash", "(D)I"),
                        _ => ("floatHash", "(F)I"),
                    };
                    self.expr(b, value)?;
                    b.code.invoke(Invoke::Static, b.pool, statics, name, d);
                }
                None => {
                    self.expr(b, qualifier)?;
                    b.code.invoke(Invoke::Static, b.pool, statics, "anyHash", "(Ljava/lang/Object;)I");
                }
            },
        }
        Ok(Some(1))
    }

    /// The letter of `Long`, `Double` or `Float` and the boxed value if `t` boxes a value whose
    /// type is that class.
    fn boxed_primitive(&mut self, t: TNodeId) -> Option<(char, TNodeId)> {
        let Tree::Apply { fun, args } = *self.l.tpd.tree(t) else { return None };
        let &[value] = self.l.tpd.items(args) else { return None };
        let m = self.l.tpd.sym(fun).filter(|&m| self.l.is_box(m))?;
        let tpe = self.l.u.widen(self.l.tpd.tpe(value));
        let class = self.l.u.class_symbol(tpe).filter(|_| self.l.u.dealias(tpe) == tpe)?;
        let letter = "jdf".chars().find(|&c| self.l.primitive(c).is_some_and(|p| p.box_method == m))?;
        let name = ["scala.Long", "scala.Double", "scala.Float"]["jdf".find(letter)?];
        (self.l.u.full_name(class) == name).then_some((letter, value))
    }

    /// Whether either side may be a boxed number, character or boolean at run time, unless both
    /// are of the same final class.
    fn must_use_any_comparator(&mut self, left: TNodeId, right: TNodeId) -> bool {
        let (l, r) = (self.erased_class(left), self.erased_class(right));
        let same_type = self.singleton(left) == self.singleton(right) && l == r;
        let same_finals = same_type && l.is_some_and(|c| self.l.u.symbol(c).flags.has(FINAL));
        !same_finals && self.is_maybe_boxed(left) && self.is_maybe_boxed(right)
    }

    /// Whether `t` erases to `Object`, `Serializable` or `Comparable`, or to a subclass of
    /// `Number`, `Character` or `Boolean`.
    fn is_maybe_boxed(&mut self, t: TNodeId) -> bool {
        let Ok(eq) = self.l.equality() else { return false };
        let Some(c) = self.erased_class(t) else { return false };
        let bases = [eq.number, eq.character, eq.boolean];
        self.l.u.binary_name(c) == "java.lang.Object"
            || [eq.serializable, eq.comparable].contains(&c)
            || bases.into_iter().any(|base| self.l.u.derives_from(c, base))
    }

    /// The class `t` erases to.
    fn erased_class(&mut self, t: TNodeId) -> Option<SymbolId> {
        let Erased::Class(name) = lower::erased_type(self.l, t) else { return None };
        let tpe = self.l.u.class_ref(&name);
        self.l.u.class_symbol(tpe)
    }

    /// `t`'s singleton type if it refers to `this`, a local value, an object or a field.
    fn singleton(&mut self, t: TNodeId) -> Option<tasty::TypeId> {
        let tpe = self.l.tpd.tpe(t);
        let singleton = match *self.l.u.ty(tpe) {
            tasty::Type::This(_) => true,
            tasty::Type::Sym(_, s) if !self.l.u.is_type_symbol(s) => {
                let in_class = self.l.u.symbol(s).owner.is_some_and(|o| self.l.u.is_class(o));
                !in_class || self.l.u.symbol(s).kind == tasty::Kind::Val || self.l.u.is_module(s)
            }
            _ => false,
        };
        singleton.then_some(tpe)
    }

    fn is_non_null(&self, t: TNodeId) -> bool {
        matches!(self.l.tpd.tree(t), Tree::Literal { .. } | Tree::ClassLiteral { .. })
            || self.l.tpd.sym(self.l.tpd.meth_part(t)).is_some_and(|s| self.l.u.is_module(s))
    }
}

fn is_null(g: &Gen, t: TNodeId) -> bool {
    matches!(g.l.tpd.tree(t), Tree::Literal { value: Constant::Null })
}
