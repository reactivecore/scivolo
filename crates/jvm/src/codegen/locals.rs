// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeSkelBuilder.scala
//! Local definitions and assignments to locals (dotty's `genStat`), and the descriptors of
//! locals and fields.

use super::body::Body;
use super::descriptors::{NULL, descriptor};
use super::{Gen, unsupported};
use classfile::Label;
use lower::Result;
use names::nme;
use syntax::ast::constant::Constant;
use tasty::tags::{MUTABLE, SYNTHETIC};
use tasty::{Erased, PRIMITIVE_VALUE_CLASSES, SymbolId};
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// Method body `rhs` returning `result`; its leading `tailrec` variables initialised from
    /// `this` or a parameter take that one's slot (dotty's `trimmedRhs`).
    pub fn method_body(&mut self, b: &mut Body, rhs: TNodeId, result: &str) -> Result<()> {
        let Tree::Block { stats, expr } = *self.l.tpd.tree(rhs) else { return self.expr_ret(b, rhs, result) };
        let stats = self.l.tpd.items(stats).to_vec();
        let mut rest = &stats[..];
        while let Some((&s, tail)) = rest.split_first()
            && let Tree::ValDef { rhs: Some(init), .. } = *self.l.tpd.tree(s)
            && let Some(sym) = self.l.tpd.sym(s)
            && self.l.u.symbol(sym).name.contains("$tailLocal")
            && self.l.u.symbol(sym).flags.has(MUTABLE)
        {
            let slot = match self.l.tpd.tree(init) {
                Tree::This { .. } => (0, "L".to_owned()),
                _ => match b.locals.iter().find(|l| Some(l.0) == self.l.tpd.sym(init)) {
                    Some(&(_, slot, ref desc)) => (slot, desc.clone()),
                    None => break,
                },
            };
            b.locals.push((sym, slot.0, slot.1));
            rest = tail;
        }
        let class_body = self.is_primary_constructor(b.method);
        b.code.open_block();
        for &s in rest {
            match class_body {
                true => self.stat(b, s)?,
                false => self.block_stat(b, s)?,
            }
        }
        let synthetic_unit = matches!(self.l.tpd.tree(expr), Tree::Literal { value: Constant::Unit })
            && self.l.tpd.span(expr) == self.l.tpd.span(rhs);
        match synthetic_unit && result == "V" {
            true => b.code.ret("V"),
            false => self.expr_ret(b, expr, result)?,
        }
        b.code.close_block();
        Ok(())
    }

    /// Names local `sym` in `slot` of descriptor `desc` from here to the end of its block unless
    /// synthetic (dotty's `varsInScope`).
    pub fn name_block_local(&mut self, b: &mut Body, sym: SymbolId, desc: &str, slot: u16) {
        if !self.l.u.symbol(sym).flags.has(SYNTHETIC) {
            let name = tasty::encode(&self.l.u.symbol(sym).name.clone());
            b.code.block_local(&name, desc, slot);
        }
    }

    /// The name of the receiver of method `sym`: `$self` of a trait initializer, none in a static
    /// method or the constructor of a primitive value class (dotty's `genDefDef`).
    pub fn this_name(&mut self, sym: SymbolId, trait_init: bool, static_: bool) -> Option<&'static str> {
        let owner = self.l.u.symbol(sym).owner.map(|o| self.l.u.full_name(o)).unwrap_or_default();
        let primitive = PRIMITIVE_VALUE_CLASSES.contains(&owner.as_str()) && self.l.u.is_constructor(sym);
        if trait_init { Some(nme::SELF) } else { (!static_ && !primitive).then_some(nme::THIS) }
    }

    /// Names the receiver in slot 0 by `this`, none in a static method, and parameters `params`
    /// from `start` to here (dotty's `emitNormalMethodBody`).
    pub fn name_this_and_params(
        &mut self,
        b: &mut Body,
        start: Label,
        this: Option<&str>,
        params: &[SymbolId],
    ) {
        if let Some((this, class)) = this.zip(b.this_class.clone()) {
            let end = b.code.mark();
            b.code.local_var(this, &format!("L{class};"), (start, end), 0);
        }
        for &p in params {
            if let Some(&(_, slot, ref desc)) = b.locals.iter().find(|l| l.0 == p) {
                let desc = desc.clone();
                self.name_local(b, p, (&desc, slot), start, true);
            }
        }
    }

    /// Names local `sym` in `slot` of descriptor `desc` from `start` to here unless synthetic
    /// and not `force` (dotty's `emitLocalVarScope`).
    pub fn name_local(
        &mut self,
        b: &mut Body,
        sym: SymbolId,
        (desc, slot): (&str, u16),
        start: Label,
        force: bool,
    ) {
        if force || !self.l.u.symbol(sym).flags.has(SYNTHETIC) {
            let name = tasty::encode(&self.l.u.symbol(sym).name.clone());
            let end = b.code.mark();
            b.code.local_var(&name, desc, (start, end), slot);
        }
    }

    /// Local definition `t` with value `rhs`, a `BoxedUnit` for a `Unit` local.
    pub fn gen_val_def(&mut self, b: &mut Body, t: TNodeId, rhs: Option<TNodeId>) -> Result<i32> {
        let sym = self.l.tpd.sym(t).expect("a local's symbol");
        let desc = self.storage_descriptor(sym);
        let Some(rhs) = rhs else { return unsupported(self.l.tpd.span(t), "a local without a value") };
        let slot = b.local(sym, desc.clone());
        if desc == NULL {
            self.expect_null(b, rhs);
        }
        self.expr_expect(b, rhs, &desc)?;
        self.cast_to_null(b, rhs, &desc);
        b.code.store(&desc, slot);
        self.name_block_local(b, sym, &desc, slot);
        Ok(0)
    }

    /// `checkcast Null$` after `t` where descriptor `desc` is `Null$` and `t` calls a method of
    /// another erased reference result (dotty's `Erasure` cast to `Null`).
    pub fn cast_to_null(&mut self, b: &mut Body, t: TNodeId, desc: &str) {
        let reference = matches!(self.call_result(t), Some(Erased::Class(_) | Erased::Array(_)));
        if desc == NULL && reference {
            b.code.checkcast(b.pool, "scala/runtime/Null$");
        }
    }

    /// The descriptor of local or field `sym`: `BoxedUnit` for a `Unit` value.
    pub fn storage_descriptor(&mut self, sym: SymbolId) -> String {
        let info = self.l.u.info(sym);
        match self.l.u.erase(info) {
            tasty::Erased::Unit => "Lscala/runtime/BoxedUnit;".to_owned(),
            e => descriptor(&e),
        }
    }

    /// Assignment `t` of `lhs = rhs` to a local, `iinc` for `x = x + c` and `x = x - c` of an int;
    /// `None` if `lhs` is not a local of this method.
    pub fn assign_local(
        &mut self,
        b: &mut Body,
        t: TNodeId,
        (lhs, rhs): (TNodeId, TNodeId),
    ) -> Result<Option<i32>> {
        let sym = self.l.tpd.sym(lhs);
        let Some(&(_, slot, ref desc)) = b.locals.iter().find(|l| Some(l.0) == sym) else { return Ok(None) };
        let desc = desc.clone();
        if let Some((name, receiver, args)) = self.primitive_call(rhs)
            && let [arg] = args[..]
            && let Tree::Literal { value } = self.l.tpd.tree(arg)
            && let Some(c) = int_like(value)
            && self.l.tpd.sym(receiver) == sym
            && matches!(name.as_str(), nme::ADD | nme::MINUS)
            && desc == "I"
            && b.code.iinc(slot, if name == nme::ADD { c } else { c.wrapping_neg() }).is_some()
        {
            return Ok(Some(0));
        }
        self.expr_expect(b, rhs, &desc)?;
        self.cast_to_null(b, rhs, &desc);
        self.line(b, t);
        b.code.store(&desc, slot);
        Ok(Some(0))
    }
}

/// The value of an int-sized literal.
pub fn int_like(value: &Constant) -> Option<i32> {
    Some(match *value {
        Constant::Int(v) => v,
        Constant::Char(v) => i32::from(v),
        Constant::Short(v) => i32::from(v),
        Constant::Byte(v) => i32::from(v),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::int_like;
    use syntax::ast::constant::Constant;

    #[test]
    fn int_sized_literals() {
        assert_eq!(int_like(&Constant::Int(i32::MIN)), Some(i32::MIN));
        assert_eq!(int_like(&Constant::Char(0xffff)), Some(65535));
        assert_eq!(int_like(&Constant::Short(-2)), Some(-2));
        assert_eq!(int_like(&Constant::Byte(-128)), Some(-128));
        assert_eq!(int_like(&Constant::Long(1)), None);
        assert_eq!(int_like(&Constant::Boolean(true)), None);
    }
}
