//! A method's `Code` attribute in ASM's order: the code's entries (`MethodNode.accept`), the
//! locals' names, then the classes of the stack map frames (`visitMaxs`).

use super::Laid;
use crate::frames::{self, Lub};
use crate::pool::{Import, Pool};
use crate::{ACC_STATIC, Code, Member, writer};

/// The `Code` attribute of `code` of method `m` of class `class`, whose entries move from the
/// pool of `im` to its target.
pub fn code(mut code: Code, class: &str, m: &Member, im: &mut Import, lub: &mut Lub) -> Result<Laid, String> {
    if let Some(e) = code.problem() {
        return Err(e);
    }
    code.remap(&mut |i| im.entry(i));
    let names = code.local_var_names(&mut im.to);
    let frames = stack_map(&mut code, class, m, &mut im.to, lub)?;

    let mut nested = Vec::new();
    if let Some(bytes) = frames {
        nested.push(Laid::new("StackMapTable", bytes));
    }
    if let Some(bytes) = code.line_table() {
        nested.push(Laid::new("LineNumberTable", bytes));
    }
    if let Some(bytes) = code.local_var_table(&names) {
        nested.push(Laid::new("LocalVariableTable", bytes));
    }
    Ok(Laid { nested: Some(nested), ..Laid::new("Code", writer::code(&code)?) })
}

/// Widens `code`'s jumps and checks its size; the body of its `StackMapTable`, none if it needs
/// none.
fn stack_map(
    code: &mut Code,
    class: &str,
    m: &Member,
    pool: &mut Pool,
    lub: &mut Lub,
) -> Result<Option<Vec<u8>>, String> {
    code.widen();
    if code.bytes.len() > usize::from(u16::MAX) {
        return Err("over 65,535 bytes of code".into());
    }
    let is_static = m.access & ACC_STATIC != 0;
    let method = frames::Method { class, is_static, name: &m.name, descriptor: &m.descriptor };
    frames::stack_map(code, pool, &method, lub)
}
