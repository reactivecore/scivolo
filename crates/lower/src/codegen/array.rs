//! Array literals: the elements, then an array allocated from them (nscplugin's
//! `genJavaSeqLiteral`).

use super::Gen;
use crate::{Result, unsupported};
use nir::{Op, Type, Val, ValId};
use typer::tpd::{List, TNodeId};

impl Gen<'_, '_> {
    /// Array literal `t` with elements `elems`: allocated from an array value if all elements are
    /// constants, not all zero; else allocated, then each element but zeros stored.
    pub fn gen_array_literal(&mut self, t: TNodeId, elems: List) -> Result<ValId> {
        let span = self.l.tpd.span(t);
        let ty = self.gen_type(self.l.tpd.tpe(t));
        let Type::Array { elem, .. } = *self.pool.ty(ty) else {
            return unsupported(span, "a sequence literal that is not an array");
        };
        let elems = self.l.tpd.items(elems).to_vec();
        let mut values = Vec::new();
        for &e in &elems {
            let v = self.gen_expr(e)?;
            self.check_conforms(v, elem, self.l.tpd.span(e))?;
            values.push(v);
        }
        let saved = self.at_span(span);
        let canonical = values.iter().all(|&v| is_canonical(self.pool.val(v)));
        if canonical && values.iter().any(|&v| !self.is_zero(v)) {
            let init = self.val(Val::ArrayValue(elem, values));
            let v = self.emit(Op::Arrayalloc { ty: elem, init, zone: None });
            self.m.pos = saved;
            return Ok(v);
        }
        let length = self.val(Val::Int(values.len() as i32));
        let arr = self.emit(Op::Arrayalloc { ty: elem, init: length, zone: None });
        for (i, (&e, &value)) in elems.iter().zip(&values).enumerate() {
            if self.is_zero(value) {
                continue;
            }
            self.at_span(self.l.tpd.span(e));
            let idx = self.val(Val::Int(i as i32));
            self.emit(Op::Arraystore { ty: elem, arr, idx, value });
        }
        self.m.pos = saved;
        Ok(arr)
    }

    /// Whether `v` is a zero value: `false`, `null`, a numeric zero (NIR's `isZero`).
    fn is_zero(&self, v: ValId) -> bool {
        match *self.pool.val(v) {
            Val::Zero(_) | Val::False | Val::Null => true,
            Val::Char(c) => c == 0,
            Val::Byte(n) => n == 0,
            Val::Short(n) => n == 0,
            Val::Int(n) => n == 0,
            Val::Long(n) => n == 0,
            Val::Size(n) => n == 0,
            Val::Float(b) => f32::from_bits(b) == 0.0,
            Val::Double(b) => f64::from_bits(b) == 0.0,
            _ => false,
        }
    }
}

/// Whether `v` is a constant an array value may hold (NIR's `isCanonical`).
fn is_canonical(v: &Val) -> bool {
    matches!(
        v,
        Val::True
            | Val::False
            | Val::Char(_)
            | Val::Size(_)
            | Val::Byte(_)
            | Val::Short(_)
            | Val::Int(_)
            | Val::Long(_)
            | Val::Float(_)
            | Val::Double(_)
            | Val::Global(..)
            | Val::Null
    )
}
