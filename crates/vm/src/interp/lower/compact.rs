//! Drops `Nop`s and jumps to the next instruction from lowered code, retargeting the rest.

use crate::interp::code::Instr;

pub fn compact(instrs: Vec<Instr>, origin: Vec<u32>) -> (Vec<Instr>, Vec<u32>) {
    let (instrs, origin) = remove(instrs, origin, |_, i| matches!(i, Instr::Nop));
    remove(
        instrs,
        origin,
        |at, i| matches!(i, Instr::Jump(t) if t.moves.is_empty() && t.pc as usize == at + 1),
    )
}

/// `instrs` without those `drop` selects; a target of a dropped instruction continues at the
/// next kept one.
fn remove(
    instrs: Vec<Instr>,
    origin: Vec<u32>,
    drop: impl Fn(usize, &Instr) -> bool,
) -> (Vec<Instr>, Vec<u32>) {
    let dropped: Vec<bool> = instrs.iter().enumerate().map(|(at, i)| drop(at, i)).collect();
    if !dropped.contains(&true) {
        return (instrs, origin);
    }
    let mut new_pc = Vec::with_capacity(instrs.len() + 1);
    let mut kept = 0u32;
    for &d in &dropped {
        new_pc.push(kept);
        kept += !d as u32;
    }
    new_pc.push(kept);
    let mut out = (Vec::new(), Vec::new());
    for ((mut instr, o), d) in instrs.into_iter().zip(origin).zip(dropped) {
        if d {
            continue;
        }
        for t in instr.targets_mut() {
            t.pc = new_pc[t.pc as usize];
        }
        out.0.push(instr);
        out.1.push(o);
    }
    out
}
