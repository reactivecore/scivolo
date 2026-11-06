//! The effect of one instruction on the verifier's types of the locals and the stack.

use super::lattice::{Frame, VType};
use crate::{Pool, method_slots, split_method, value_slots};

const KINDS: [VType; 4] = [VType::Int, VType::Long, VType::Float, VType::Double];

/// The length of the instruction at `at`.
pub fn length(code: &[u8], at: usize) -> usize {
    match code[at] {
        0x10 | 0x12 | 0x15..=0x19 | 0x36..=0x3a | 0xbc => 2,
        0x11 | 0x13 | 0x14 | 0x84 | 0x99..=0xa7 | 0xb2..=0xb8 | 0xbb | 0xbd | 0xc0 | 0xc1 | 0xc6 | 0xc7 => 3,
        0xb9 | 0xba => 5,
        0xc4 if code[at + 1] == 0x84 => 6,
        0xc4 => 4,
        _ => 1,
    }
}

/// The target of the jump at `at`, if it is one.
pub fn target(code: &[u8], at: usize) -> Option<usize> {
    matches!(code[at], 0x99..=0xa7 | 0xc6 | 0xc7).then(|| {
        let offset = i16::from_be_bytes([code[at + 1], code[at + 2]]);
        (at as i64 + i64::from(offset)) as usize
    })
}

fn u16_at(code: &[u8], i: usize) -> u16 {
    u16::from_be_bytes([code[i], code[i + 1]])
}

fn size(t: &VType) -> usize {
    if t.wide() { 2 } else { 1 }
}

/// Runs the instruction at `at` of a method of class `this` on `f`.
pub fn execute(f: &mut Frame, code: &[u8], at: usize, pool: &Pool, this: &str) -> Result<(), String> {
    let op = code[at];
    match op {
        0x01 => f.push(VType::Null),
        0x02..=0x08 | 0x10 | 0x11 => f.push(VType::Int),
        0x09 | 0x0a => f.push(VType::Long),
        0x0b..=0x0d => f.push(VType::Float),
        0x0e | 0x0f => f.push(VType::Double),
        0x12 => f.push_desc(pool.constant_descriptor(code[at + 1].into())),
        0x13 | 0x14 => f.push_desc(pool.constant_descriptor(u16_at(code, at + 1))),
        0x15..=0x19 => load(f, op - 0x15, code[at + 1].into())?,
        0x1a..=0x2d => load(f, (op - 0x1a) / 4, ((op - 0x1a) % 4).into())?,
        0x2e..=0x35 => {
            f.pop(1)?;
            let array = f.pop(1)?;
            f.push(match (op, array) {
                (0x32, VType::Object(a)) => {
                    a.strip_prefix('[').and_then(VType::of).ok_or("aaload of no array")?
                }
                (0x32, _) => VType::Null,
                (0x2f, _) => VType::Long,
                (0x30, _) => VType::Float,
                (0x31, _) => VType::Double,
                _ => VType::Int,
            })
        }
        0x36..=0x3a => store(f, op - 0x36, code[at + 1].into())?,
        0x3b..=0x4e => store(f, (op - 0x3b) / 4, ((op - 0x3b) % 4).into())?,
        0x4f..=0x56 => drop(f.pop(if matches!(op, 0x50 | 0x52) { 4 } else { 3 })?),
        0x57 | 0x58 => drop(f.pop(usize::from(op - 0x56))?),
        0x59 | 0x5c => {
            let n = usize::from(op == 0x5c) + 1;
            let from = f.stack.len().checked_sub(n).ok_or("stack underflow")?;
            f.stack.extend_from_within(from..);
        }
        0x60..=0x77 => {
            let t = KINDS[usize::from(op - 0x60) % 4].clone();
            f.pop(size(&t) * if op < 0x74 { 2 } else { 1 })?;
            f.push(t);
        }
        0x78..=0x83 => {
            let t = KINDS[usize::from(op - 0x78) % 2].clone();
            f.pop(if op < 0x7e { 1 + size(&t) } else { 2 * size(&t) })?;
            f.push(t);
        }
        0x84 | 0xa7 | 0xac..=0xb1 | 0xbf => {}
        0x85..=0x90 => {
            let from = usize::from(op - 0x85) / 3;
            let mut to: Vec<VType> = KINDS.to_vec();
            to.remove(from);
            f.pop(size(&KINDS[from]))?;
            f.push(to[usize::from(op - 0x85) % 3].clone());
        }
        0x91..=0x98 => {
            f.pop(match op {
                0x94 | 0x97 | 0x98 => 4,
                0x95 | 0x96 => 2,
                _ => 1,
            })?;
            f.push(VType::Int);
        }
        0x99..=0x9e | 0xc6 | 0xc7 => drop(f.pop(1)?),
        0x9f..=0xa6 => drop(f.pop(2)?),
        0xb2..=0xb5 => {
            let (_, _, d) = pool.member_parts(u16_at(code, at + 1));
            let slots = value_slots(&d) as usize;
            match op {
                0xb2 => f.push_desc(&d),
                0xb3 => drop(f.pop(slots)?),
                0xb4 => {
                    f.pop(1)?;
                    f.push_desc(&d)
                }
                _ => drop(f.pop(slots + 1)?),
            }
        }
        0xb6..=0xba => invoke(f, code, at, pool, this)?,
        0xbb => f.push(VType::Uninit(at as u16)),
        0xbc => {
            f.pop(1)?;
            let elem =
                ["Z", "C", "F", "D", "B", "S", "I", "J"].get(usize::from(code[at + 1]).wrapping_sub(4));
            f.push(VType::Object(format!("[{}", elem.ok_or("newarray of no primitive type")?)));
        }
        0xbd => {
            f.pop(1)?;
            let c = pool.class_name(u16_at(code, at + 1));
            f.push(VType::Object(if c.starts_with('[') { format!("[{c}") } else { format!("[L{c};") }));
        }
        0xbe | 0xc1 => {
            f.pop(1)?;
            f.push(VType::Int)
        }
        0xc0 => {
            f.pop(1)?;
            f.push(VType::Object(pool.class_name(u16_at(code, at + 1))))
        }
        0xc4 => match code[at + 1] {
            0x84 => {}
            o @ 0x15..=0x19 => load(f, o - 0x15, u16_at(code, at + 2))?,
            o @ 0x36..=0x3a => store(f, o - 0x36, u16_at(code, at + 2))?,
            o => return Err(format!("wide opcode {o:#x}")),
        },
        _ => return Err(format!("opcode {op:#x} in stack map frames")),
    }
    Ok(())
}

/// A load of kind `kind` (`i l f d a`) from `slot`.
fn load(f: &mut Frame, kind: u8, slot: u16) -> Result<(), String> {
    let t = match KINDS.get(usize::from(kind)) {
        Some(t) => t.clone(),
        None => f.locals.get(usize::from(slot)).cloned().ok_or("a load past max_locals")?,
    };
    f.push(t);
    Ok(())
}

/// A store of kind `kind` (`i l f d a`) into `slot`.
fn store(f: &mut Frame, kind: u8, slot: u16) -> Result<(), String> {
    let slot = usize::from(slot);
    let t = match KINDS.get(usize::from(kind)) {
        Some(t) => {
            f.pop(size(t))?;
            t.clone()
        }
        None => f.pop(1)?,
    };
    if slot + size(&t) > f.locals.len() {
        return Err("a store past max_locals".into());
    }
    if slot > 0 && f.locals[slot - 1].wide() {
        f.locals[slot - 1] = VType::Top;
    }
    if t.wide() {
        f.locals[slot + 1] = VType::Top;
    }
    f.locals[slot] = t;
    Ok(())
}

/// An `invoke…` at `at`; a constructor call initializes its receiver everywhere in `f`.
fn invoke(f: &mut Frame, code: &[u8], at: usize, pool: &Pool, this: &str) -> Result<(), String> {
    let (_, name, d) = pool.member_parts(u16_at(code, at + 1));
    f.pop(method_slots(&d).0 as usize)?;
    if !matches!(code[at], 0xb8 | 0xba) {
        let receiver = f.pop(1)?;
        if name == "<init>" {
            let class = match receiver {
                VType::UninitThis => this.to_owned(),
                VType::Uninit(o) => pool.class_name(u16_at(code, usize::from(o) + 1)),
                _ => return Err("a constructor call on an initialized object".into()),
            };
            let init = VType::Object(class);
            for t in f.locals.iter_mut().chain(f.stack.iter_mut()).filter(|t| **t == receiver) {
                *t = init.clone();
            }
        }
    }
    f.push_desc(split_method(&d).1);
    Ok(())
}
