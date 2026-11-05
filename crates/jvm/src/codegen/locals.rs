// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeSkelBuilder.scala
//! Local definitions and assignments to locals (dotty's `genStat`).

use super::method::Body;
use super::{Gen, descriptor, unsupported};
use lower::Result;
use syntax::ast::constant::Constant;
use tasty::tags::MUTABLE;
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
        for &s in rest {
            self.stat(b, s)?;
        }
        self.expr_ret(b, expr, result)
    }

    /// Local definition `t` with value `rhs`.
    pub fn gen_val_def(&mut self, b: &mut Body, t: TNodeId, rhs: Option<TNodeId>) -> Result<i32> {
        let sym = self.l.tpd.sym(t).expect("a local's symbol");
        let info = self.l.u.info(sym);
        let desc = descriptor(&self.l.u.erase(info));
        let Some(rhs) = rhs else { return unsupported(self.l.tpd.span(t), "a local without a value") };
        let slot = b.bind(sym, desc.clone());
        self.expr_as(b, rhs, classfile::value_slots(&desc))?;
        b.code.store(&desc, slot);
        Ok(0)
    }

    /// Assignment `lhs = rhs` to a local, `iinc` for `x = x + c` and `x = x - c` of an int; `None`
    /// if `lhs` is not a local of this method.
    pub fn assign_local(&mut self, b: &mut Body, lhs: TNodeId, rhs: TNodeId) -> Result<Option<i32>> {
        let sym = self.l.tpd.sym(lhs);
        let Some(&(_, slot, ref desc)) = b.locals.iter().find(|l| Some(l.0) == sym) else { return Ok(None) };
        let desc = desc.clone();
        if let Some((name, receiver, args)) = self.primitive_call(rhs)
            && let [arg] = args[..]
            && let Tree::Literal { value } = self.l.tpd.tree(arg)
            && let Some(c) = int_like(value)
            && self.l.tpd.sym(receiver) == sym
            && matches!(name.as_str(), "+" | "-")
            && desc == "I"
            && b.code.iinc(slot, if name == "+" { c } else { c.wrapping_neg() }).is_some()
        {
            return Ok(Some(0));
        }
        self.expr_as(b, rhs, classfile::value_slots(&desc))?;
        b.code.store(&desc, slot);
        Ok(Some(0))
    }
}

/// The value of an int-sized literal.
fn int_like(value: &Constant) -> Option<i32> {
    Some(match *value {
        Constant::Int(v) => v,
        Constant::Char(v) => i32::from(v),
        Constant::Short(v) => i32::from(v),
        Constant::Byte(v) => i32::from(v),
        _ => return None,
    })
}
