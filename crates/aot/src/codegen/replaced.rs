//! Library methods whose bodies the codegen writes, as the VM replaces them natively: the stack
//! trace element of a frame.

use super::body::FnBuilder;
use crate::R;
use cranelift_codegen::ir::InstBuilder;
use nir::{GlobalId, Pool};

pub enum Replaced {
    Element,
}

pub fn replaced(pool: &Pool, g: GlobalId) -> Option<Replaced> {
    let (owner, sig) = (pool.top_name(g), pool.member_sig(g)?);
    match owner {
        _ if vm::is_element_method(owner, sig) => Some(Replaced::Element),
        _ => None,
    }
}

impl FnBuilder<'_, '_> {
    pub fn replaced_body(&mut self, replaced: Replaced) -> R<()> {
        let block = self.b.create_block();
        self.b.append_block_params_for_function_params(block);
        self.b.switch_to_block(block);
        let params = self.b.block_params(block).to_vec();
        let result = match replaced {
            Replaced::Element => self.trace_element(params[1])?,
        };
        self.b.ins().return_(&[result]);
        Ok(())
    }
}
