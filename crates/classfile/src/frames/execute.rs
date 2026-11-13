//! The effect of one instruction on the verifier's types of the locals and the stack.

use super::lattice::{Frame, VType};
use crate::{Pool, method_slots, split_method, value_slots};

const KINDS: [VType; 4] = [VType::Int, VType::Long, VType::Float, VType::Double];

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
        0x00 | 0x84 | 0xa7 | 0xac..=0xb1 | 0xbf | 0xc8 => {}
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
        0x99..=0x9e | 0xaa | 0xab | 0xc2 | 0xc3 | 0xc6 | 0xc7 => drop(f.pop(1)?),
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

#[cfg(test)]
mod tests {
    use super::super::flow::length;
    use super::{Frame, VType, execute};
    use crate::{Pool, Ref};
    use VType::*;

    fn obj(name: &str) -> VType {
        VType::Object(name.into())
    }

    /// Runs `code` of a method of class `a/C` on a frame of `locals` and `stack`.
    fn run(pool: &Pool, code: &[u8], locals: &[VType], stack: &[VType]) -> Result<Frame, String> {
        let mut f = Frame { locals: locals.to_vec(), stack: stack.to_vec() };
        let mut at = 0;
        while at < code.len() {
            execute(&mut f, code, at, pool, "a/C")?;
            at += length(code, at);
        }
        Ok(f)
    }

    /// The opcode `op` followed by pool index `i`.
    fn with(op: u8, i: u16) -> Vec<u8> {
        [&[op][..], &i.to_be_bytes()].concat()
    }

    #[test]
    fn instructions_change_the_stack_by_their_types() {
        let mut pool = Pool::default();
        let string = pool.string(&[0x61]);
        let long = pool.long(7);
        let class = pool.class("a/D");
        let strings = pool.class("java/lang/String");
        let ints = pool.class("[I");
        let field = pool.member(Ref::Field, "a/C", "x", "J");
        let method = pool.member(Ref::Method, "a/C", "m", "(IJ)D");
        let interface = pool.member(Ref::InterfaceMethod, "a/I", "n", "(I)La/D;");
        let locals = [Int, Float, Long, Top, obj("a/D")];
        let cases: Vec<(Vec<u8>, Vec<VType>, Vec<VType>)> = vec![
            (vec![0x01], vec![], vec![Null]),
            (vec![0x02, 0x11, 1, 0], vec![], vec![Int, Int]),
            (vec![0x0a, 0x0c, 0x0f], vec![], vec![Long, Top, Float, Double, Top]),
            (vec![0x1a, 0x23, 0x20], vec![], vec![Int, Float, Long, Top]),
            (vec![0x19, 4], vec![], vec![obj("a/D")]),
            (vec![0x61], vec![Long, Top, Long, Top], vec![Long, Top]),
            (vec![0x79], vec![Long, Top, Int], vec![Long, Top]),
            (vec![0x7e], vec![Int, Int], vec![Int]),
            (vec![0x7f], vec![Long, Top, Long, Top], vec![Long, Top]),
            (vec![0x6a], vec![Float, Float], vec![Float]),
            (vec![0x77], vec![Double, Top], vec![Double, Top]),
            (vec![0x85], vec![Int], vec![Long, Top]),
            (vec![0x88], vec![Long, Top], vec![Int]),
            (vec![0x90], vec![Double, Top], vec![Float]),
            (vec![0x8d], vec![Float], vec![Double, Top]),
            (vec![0x92], vec![Int], vec![Int]),
            (vec![0x94], vec![Long, Top, Long, Top], vec![Int]),
            (vec![0x96], vec![Float, Float], vec![Int]),
            (vec![0x97], vec![Double, Top, Double, Top], vec![Int]),
            (vec![0x32], vec![obj("[[I"), Int], vec![obj("[I")]),
            (vec![0x32], vec![Null, Int], vec![Null]),
            (vec![0x2f], vec![obj("[J"), Int], vec![Long, Top]),
            (vec![0x33], vec![obj("[Z"), Int], vec![Int]),
            (vec![0x50], vec![obj("[J"), Int, Long, Top], vec![]),
            (vec![0xbc, 10], vec![Int], vec![obj("[I")]),
            (vec![0xbc, 11], vec![Int], vec![obj("[J")]),
            (with(0xbd, strings), vec![Int], vec![obj("[Ljava/lang/String;")]),
            (with(0xbd, ints), vec![Int], vec![obj("[[I")]),
            (vec![0xbe], vec![obj("[I")], vec![Int]),
            (vec![0x5c], vec![Long, Top], vec![Long, Top, Long, Top]),
            (vec![0x59], vec![Int], vec![Int, Int]),
            (vec![0x58], vec![Double, Top], vec![]),
            (with(0xc0, class), vec![obj("java/lang/Object")], vec![obj("a/D")]),
            (with(0xc1, class), vec![Null], vec![Int]),
            (vec![0x99, 0, 3], vec![Int], vec![]),
            (vec![0xa0, 0, 3], vec![Int, Int], vec![]),
            (vec![0xc6, 0, 3], vec![Null], vec![]),
            (vec![0x12, string as u8], vec![], vec![obj("java/lang/String")]),
            (with(0x14, long), vec![], vec![Long, Top]),
            (vec![0x12, class as u8], vec![], vec![obj("java/lang/Class")]),
            (with(0xb4, field), vec![obj("a/C")], vec![Long, Top]),
            (with(0xb2, field), vec![], vec![Long, Top]),
            (with(0xb3, field), vec![Long, Top], vec![]),
            (with(0xb5, field), vec![obj("a/C"), Long, Top], vec![]),
            (with(0xb6, method), vec![obj("a/C"), Int, Long, Top], vec![Double, Top]),
            (with(0xb8, method), vec![Int, Long, Top], vec![Double, Top]),
            ([with(0xb9, interface), vec![2, 0]].concat(), vec![obj("a/I"), Int], vec![obj("a/D")]),
            (vec![0xc4, 0x19, 0, 4], vec![], vec![obj("a/D")]),
            (vec![0xc4, 0x84, 0, 0, 0, 1, 0x84, 0, 1], vec![], vec![]),
        ];
        for (code, before, after) in cases {
            let f = run(&pool, &code, &locals, &before).map_err(|e| format!("{code:x?}: {e}"));
            assert_eq!(f.map(|f| f.stack), Ok(after), "{code:x?}");
        }
    }

    #[test]
    fn stores_set_locals_and_break_wide_values() {
        let pool = Pool::default();
        let stored = |code: &[u8], locals: &[VType], stack: &[VType]| {
            run(&pool, code, locals, stack).map(|f| f.locals)
        };
        assert_eq!(stored(&[0x3c], &[Long, Top, Int], &[Int]), Ok(vec![Top, Int, Int]));
        assert_eq!(stored(&[0x47], &[Int, Int, Int], &[Double, Top]), Ok(vec![Double, Top, Int]));
        assert_eq!(stored(&[0x3a, 2], &[Int, Int, Int], &[Null]), Ok(vec![Int, Int, Null]));
        assert_eq!(stored(&[0xc4, 0x37, 0, 1], &[Int, Int, Int], &[Long, Top]), Ok(vec![Int, Long, Top]));
        assert!(stored(&[0x3f], &[Int], &[Long, Top]).unwrap_err().contains("past max_locals"));
        assert!(stored(&[0x19, 3], &[Int], &[]).unwrap_err().contains("past max_locals"));
    }

    #[test]
    fn constructor_calls_initialize_every_copy_of_their_receiver() {
        let mut pool = Pool::default();
        let class = pool.class("a/D");
        let init = pool.member(Ref::Method, "a/D", "<init>", "(I)V");
        let object_init = pool.member(Ref::Method, "java/lang/Object", "<init>", "()V");
        let code = [with(0xbb, class), vec![0x59, 0x4b, 0x59, 0x04], with(0xb7, init)].concat();
        let f = run(&pool, &code, &[Top], &[]).expect("a frame");
        assert_eq!((f.locals, f.stack), (vec![obj("a/D")], vec![obj("a/D")]));
        let f = run(&pool, &[vec![0x2a], with(0xb7, object_init)].concat(), &[UninitThis], &[]);
        assert_eq!(f.map(|f| f.locals), Ok(vec![obj("a/C")]));
        let twice = run(&pool, &with(0xb7, object_init), &[], &[obj("a/D")]);
        assert!(twice.unwrap_err().contains("initialized object"));
    }

    #[test]
    fn unsupported_and_ill_typed_instructions_are_refused() {
        let pool = Pool::default();
        let cases: [(&[u8], &[VType], &str); 5] = [
            (&[0x5a], &[Int, Int], "opcode 0x5a"),
            (&[0x57], &[], "stack underflow"),
            (&[0x32], &[obj("a/D"), Int], "aaload of no array"),
            (&[0xbc, 12], &[Int], "newarray of no primitive type"),
            (&[0xc4, 0x10, 0, 0], &[], "wide opcode 0x10"),
        ];
        for (code, stack, error) in cases {
            let e = run(&pool, code, &[], stack).unwrap_err();
            assert!(e.contains(error), "{code:x?}: {e}");
        }
    }
}
