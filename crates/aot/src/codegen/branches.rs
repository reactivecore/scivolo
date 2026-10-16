//! Conditional branches: `if` and `switch`.

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::condcodes::IntCC;
use nir::{Next, ValId};

impl FnBuilder<'_, '_> {
    /// `if c then … else …`; when both go to one block, the else side passes through a block of
    /// its own.
    pub fn branch(&mut self, c: ValId, then: &Next, els: &Next) -> R<()> {
        let c = self.val(c)?;
        let (then, then_args) = self.target(then)?;
        let (els, els_args) = self.target(els)?;
        if then == els {
            let via = self.b.create_block();
            self.b.ins().brif(c, then, &then_args, via, &[]);
            self.b.switch_to_block(via);
            self.b.ins().jump(els, &els_args);
        } else {
            self.b.ins().brif(c, then, &then_args, els, &els_args);
        }
        Ok(())
    }

    /// `switch scrut`: one comparison per case, then the default.
    pub fn switch(&mut self, scrut: ValId, default: &Next, cases: &[Next]) -> R<()> {
        let s = self.val(scrut)?;
        for case in cases {
            let Next::Case(v, to) = case else { return Err("switch case".into()) };
            let v = self.val(*v)?;
            let eq = self.b.ins().icmp(IntCC::Equal, s, v);
            let (block, args) = self.target(to)?;
            let next = self.b.create_block();
            self.b.ins().brif(eq, block, &args, next, &[]);
            self.b.switch_to_block(next);
        }
        self.jump(default)
    }
}
