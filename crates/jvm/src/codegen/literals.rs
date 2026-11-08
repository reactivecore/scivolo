// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! Literals and the primitives' `box` and `unbox` (dotty's `genConstant`, `genApply` of a box).

use super::body::Body;
use super::{Gen, class_internal, primitive_box, unsupported};
use classfile::{FieldOp, Invoke};
use lower::Result;
use syntax::ast::constant::Constant;
use tasty::Erased;
use typer::tpd::TNodeId;

impl Gen<'_, '_> {
    /// Literal `value`; returns its slots.
    pub fn literal(&mut self, b: &mut Body, value: &Constant) -> i32 {
        match *value {
            Constant::Boolean(v) => b.code.int(b.pool, i32::from(v)),
            Constant::Byte(v) => b.code.int(b.pool, i32::from(v)),
            Constant::Short(v) => b.code.int(b.pool, i32::from(v)),
            Constant::Char(v) => b.code.int(b.pool, i32::from(v)),
            Constant::Int(v) => b.code.int(b.pool, v),
            Constant::Long(v) => {
                b.code.long(b.pool, v);
                return 2;
            }
            Constant::Float(v) => b.code.float(b.pool, v),
            Constant::Double(v) => {
                b.code.double(b.pool, v);
                return 2;
            }
            Constant::Unit => return 0,
            Constant::Null => b.code.aconst_null(),
            Constant::String(ref s) => {
                let index = b.pool.string(s);
                b.code.ldc(index);
            }
        }
        1
    }

    /// `classOf[T]` of type `tpe`: a primitive's `TYPE` field, else an `ldc` of the class.
    pub fn class_literal(&mut self, b: &mut Body, tpe: tasty::TypeId) -> i32 {
        let e = self.l.u.erase_boxed(tpe);
        match primitive_box(&e) {
            Some(class) => b.code.field(FieldOp::GetStatic, b.pool, class, "TYPE", "Ljava/lang/Class;"),
            None => {
                let index = b.pool.class(&class_internal(&e));
                b.code.ldc(index);
            }
        }
        1
    }

    /// Loads `t`, of a value class's underlying type, a primitive type or `Unit`, boxed: in an
    /// instance of the value class, through `BoxesRunTime`, or as `BoxedUnit.UNIT`.
    pub fn box_value(&mut self, b: &mut Body, t: TNodeId) -> Result<()> {
        let erased = lower::erased_type(self.l, t);
        let class = match erased {
            Erased::Value(ref name, _) => {
                let tpe = self.l.u.class_ref(name);
                self.l.u.value_class(tpe)
            }
            _ => None,
        };
        let Some(c) = class else {
            let letter = match erased {
                Erased::Prim(c) => c,
                _ => {
                    self.expr_as(b, t, 1)?;
                    return Ok(());
                }
            };
            self.expr(b, t)?;
            let i = "zcbsijfd".find(letter).expect("a primitive letter");
            let class = ["Boolean", "Character", "Byte", "Short", "Integer", "Long", "Float", "Double"][i];
            let d = format!("({})Ljava/lang/{class};", &"ZCBSIJFD"[i..=i]);
            b.code.invoke(Invoke::Static, b.pool, "scala/runtime/BoxesRunTime", &format!("boxTo{class}"), &d);
            return Ok(());
        };
        let name = self.class_name(c);
        b.code.new_object(b.pool, &name);
        b.code.dup();
        self.expr(b, t)?;
        let desc = format!("({})V", self.desc_of(t));
        b.code.invoke(Invoke::Special, b.pool, &name, "<init>", &desc);
        Ok(())
    }

    /// A call of a primitive's `box` or `unbox` with `args`, through `BoxesRunTime`; `None` for
    /// another method.
    pub fn boxing(&mut self, b: &mut Body, m: tasty::SymbolId, args: &[TNodeId]) -> Result<Option<i32>> {
        let (unbox, boxed) = (self.l.unboxed_by(m), self.l.is_box(m));
        let ([arg], true) = (args, boxed || unbox.is_some()) else { return Ok(None) };
        if boxed && self.l.u.erase(self.l.tpd.tpe(*arg)) == Erased::Unit {
            self.expr_as(b, *arg, 0)?;
            b.code.field(
                FieldOp::GetStatic,
                b.pool,
                "scala/runtime/BoxedUnit",
                "UNIT",
                "Lscala/runtime/BoxedUnit;",
            );
            return Ok(Some(1));
        }
        self.expr(b, *arg)?;
        let letter = match unbox {
            Some(p) => p.letter,
            None => match self.l.u.erase(self.l.tpd.tpe(*arg)) {
                Erased::Prim(c) => c,
                _ => return unsupported(self.l.tpd.span(*arg), "a box of a value that is not primitive"),
            },
        };
        let i = "zcbsijfd".find(letter).expect("a primitive letter");
        let desc = &"ZCBSIJFD"[i..=i];
        let class = ["Boolean", "Character", "Byte", "Short", "Integer", "Long", "Float", "Double"][i];
        let (name, d) = match unbox {
            Some(_) => {
                let short = ["Boolean", "Char", "Byte", "Short", "Int", "Long", "Float", "Double"][i];
                (format!("unboxTo{short}"), format!("(Ljava/lang/Object;){desc}"))
            }
            None => (format!("boxTo{class}"), format!("({desc})Ljava/lang/{class};")),
        };
        b.code.invoke(Invoke::Static, b.pool, "scala/runtime/BoxesRunTime", &name, &d);
        Ok(Some(classfile::method_slots(&d).1))
    }
}
