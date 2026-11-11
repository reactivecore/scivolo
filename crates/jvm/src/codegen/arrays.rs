// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! Arrays and casts (dotty's `genArrayOp`, `genArrayValue`, `genTypeApply`).

use super::body::Body;
use super::descriptors::{class_internal, descriptor};
use super::{Gen, unsupported};
use lower::Result;
use names::nme;
use typer::tpd::{List, TNodeId, Tree};

impl Gen<'_, '_> {
    /// Call `t` of `fun` with `args` if it is `new Array[T](n)` or an array's `apply`, `update` or
    /// `length`; `None` for another call.
    pub fn array_op(&mut self, b: &mut Body, t: TNodeId, fun: TNodeId, args: List) -> Result<Option<i32>> {
        let Some(m) = self.l.tpd.sym(fun) else { return Ok(None) };
        let owner = self.l.u.symbol(m).owner.map(|o| self.l.u.binary_name(o));
        let Tree::Select { qualifier, .. } = *self.l.tpd.tree(fun) else { return Ok(None) };
        if owner.as_deref() != Some("scala.Array") {
            return Ok(None);
        }
        let args = self.l.tpd.items(args).to_vec();
        let name = self.l.u.symbol(m).name.clone();
        let array = match *self.l.tpd.tree(qualifier) {
            Tree::New { .. } => t,
            _ => qualifier,
        };
        let desc = self.desc_of(array);
        let Some(elem) = desc.strip_prefix('[').map(str::to_owned) else {
            return unsupported(self.l.tpd.span(t), "operations on generic arrays");
        };
        if array == t {
            let [length] = args[..] else {
                return unsupported(self.l.tpd.span(t), "arrays of several dimensions");
            };
            self.operand(b, length, 'I')?;
            b.code.new_array(b.pool, &elem);
            b.code.checkcast(b.pool, &desc);
            return Ok(Some(1));
        }
        self.expr(b, array)?;
        Ok(Some(match (name.as_str(), &args[..]) {
            (nme::APPLY, &[i]) => {
                self.operand(b, i, 'I')?;
                b.code.element(&elem, false);
                classfile::value_slots(&elem)
            }
            (nme::UPDATE, &[i, value]) => {
                self.operand(b, i, 'I')?;
                self.expr_as(b, value, classfile::value_slots(&elem))?;
                b.code.element(&elem, true);
                0
            }
            (nme::LENGTH, []) => {
                b.code.insn(0xbe);
                1
            }
            _ => return unsupported(self.l.tpd.span(t), "array operations"),
        }))
    }

    /// Array literal `t` of `elems`: the array, then each element stored.
    pub fn array_literal(&mut self, b: &mut Body, t: TNodeId, elems: List) -> Result<i32> {
        let desc = self.desc_of(t);
        let Some(elem) = desc.strip_prefix('[').map(str::to_owned) else {
            return unsupported(self.l.tpd.span(t), "a sequence literal that is not an array");
        };
        let elems = self.l.tpd.items(elems).to_vec();
        let length = i32::try_from(elems.len()).unwrap_or(i32::MAX);
        b.push("L");
        b.code.int(b.pool, length);
        b.code.new_array(b.pool, &elem);
        for (i, e) in elems.into_iter().enumerate() {
            b.push("L");
            b.code.insn(0x59);
            b.push("I");
            b.code.int(b.pool, i as i32);
            self.expr_as(b, e, classfile::value_slots(&elem))?;
            b.code.element(&elem, true);
        }
        Ok(1)
    }

    /// `qualifier.isInstanceOf[T]` or `qualifier.asInstanceOf[T]` of type application `t`.
    pub fn type_test(&mut self, b: &mut Body, t: TNodeId, fun: TNodeId, targs: List) -> Result<i32> {
        let span = self.l.tpd.span(t);
        let (Tree::Select { qualifier, .. }, Some(m)) = (self.l.tpd.tree(fun).clone(), self.l.tpd.sym(fun))
        else {
            return unsupported(span, "type applications without a call");
        };
        let &[targ] = self.l.tpd.items(targs) else {
            return unsupported(span, "a type test of several types");
        };
        let tested = self.l.u.erase_boxed(self.l.tpd.tpe(targ));
        let from = self.desc_of(qualifier);
        let to = descriptor(&tested);
        let reference = |d: &str| d.starts_with('L') || d.starts_with('[');
        use tasty::Erased::{Array, Class, Nothing, Null};
        let class_or_array = matches!(tested, Class(_) | Array(_) | Nothing | Null);
        if !reference(&to) || !class_or_array {
            return unsupported(span, "type tests and casts to primitive types");
        }
        match reference(&from) {
            true => _ = self.expr(b, qualifier)?,
            false => self.box_value(b, qualifier)?,
        }
        match self.l.u.symbol(m).name.as_str() {
            nme::IS_INSTANCE_OF => b.code.instance_of(b.pool, &class_internal(&tested)),
            nme::AS_INSTANCE_OF => {
                b.code.checkcast(b.pool, &class_internal(&tested));
                super::callee::adapt_phantom(b, &to);
            }
            _ => return unsupported(span, "type applications without a call"),
        }
        Ok(1)
    }
}
