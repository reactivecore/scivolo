//! The compressed `StackMapTable` encoding (JVMS §4.7.4).

use super::lattice::{Frame, VType};
use crate::Pool;

/// The verification types of `slots`, a long or double as one; trailing `Top`s dropped if `trim`.
fn entries(slots: &[VType], trim: bool) -> Vec<VType> {
    let (mut out, mut i) = (Vec::new(), 0);
    while let Some(t) = slots.get(i) {
        out.push(t.clone());
        i += if t.wide() { 2 } else { 1 };
    }
    while trim && out.last() == Some(&VType::Top) {
        out.pop();
    }
    out
}

fn write_type(out: &mut Vec<u8>, t: &VType, pool: &mut Pool) {
    match t {
        VType::Top => out.push(0),
        VType::Int => out.push(1),
        VType::Float => out.push(2),
        VType::Double => out.push(3),
        VType::Long => out.push(4),
        VType::Null => out.push(5),
        VType::UninitThis => out.push(6),
        VType::Object(c) => {
            out.push(7);
            out.extend(pool.class(c).to_be_bytes());
        }
        VType::Uninit(at) => {
            out.push(8);
            out.extend(at.to_be_bytes());
        }
    }
}

fn write_types(out: &mut Vec<u8>, ts: &[VType], pool: &mut Pool) {
    out.extend((ts.len() as u16).to_be_bytes());
    for t in ts {
        write_type(out, t, pool);
    }
}

/// The table of `frames`, each at its offset, after the method's `initial` frame.
pub fn stack_map_table(frames: &[(usize, Frame)], initial: &Frame, pool: &mut Pool) -> Vec<u8> {
    let mut out = (frames.len() as u16).to_be_bytes().to_vec();
    let mut prev = entries(&initial.locals, true);
    let mut prev_at = None;
    for (at, f) in frames {
        let delta = prev_at.map_or(*at, |p: usize| at - p - 1) as u16;
        prev_at = Some(*at);
        let (locals, stack) = (entries(&f.locals, true), entries(&f.stack, false));
        let same = locals == prev;
        let (fewer, more) =
            (prev.len().saturating_sub(locals.len()), locals.len().saturating_sub(prev.len()));
        match stack.len() {
            0 if same && delta < 64 => out.push(delta as u8),
            0 if same => {
                out.push(251);
                out.extend(delta.to_be_bytes());
            }
            1 if same => {
                match delta < 63 {
                    true => out.push(64 + delta as u8),
                    false => {
                        out.push(247);
                        out.extend(delta.to_be_bytes());
                    }
                }
                write_type(&mut out, &stack[0], pool);
            }
            0 if (1..=3).contains(&fewer) && prev.starts_with(&locals) => {
                out.push(251 - fewer as u8);
                out.extend(delta.to_be_bytes());
            }
            0 if (1..=3).contains(&more) && locals.starts_with(&prev) => {
                out.push(251 + more as u8);
                out.extend(delta.to_be_bytes());
                for t in &locals[prev.len()..] {
                    write_type(&mut out, t, pool);
                }
            }
            _ => {
                out.push(255);
                out.extend(delta.to_be_bytes());
                write_types(&mut out, &locals, pool);
                write_types(&mut out, &stack, pool);
            }
        }
        prev = locals;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Frame, VType, stack_map_table};
    use crate::Pool;
    use VType::*;

    fn frame(locals: &[VType], stack: &[VType]) -> Frame {
        Frame { locals: locals.to_vec(), stack: stack.to_vec() }
    }

    #[test]
    fn frames_take_their_shortest_kind() {
        let mut pool = Pool::default();
        let frames = [
            (5, frame(&[Int, Top], &[])),
            (6, frame(&[Int], &[Object("a/A".into())])),
            (70, frame(&[Int], &[Int])),
            (100, frame(&[Int, Long, Top, Top], &[])),
            (101, frame(&[Int, Top, Top], &[])),
            (200, frame(&[Int], &[])),
            (300, frame(&[Int], &[Long, Top])),
            (301, frame(&[Float, Null, Top], &[Int])),
            (302, frame(&[], &[])),
            (303, frame(&[Int, Double, Top, UninitThis, Uninit(9)], &[])),
            (304, frame(&[Int, Double, Top, UninitThis, Uninit(9)], &[])),
        ];
        let table = stack_map_table(&frames, &frame(&[Int, Top], &[]), &mut pool);
        let [a0, a1] = pool.class("a/A").to_be_bytes();
        let expected = [
            &[0, 11][..],
            &[5],
            &[64, 7, a0, a1],
            &[247, 0, 63, 1],
            &[252, 0, 29, 4],
            &[250, 0, 0],
            &[251, 0, 98],
            &[247, 0, 99, 4],
            &[255, 0, 0, 0, 2, 2, 5, 0, 1, 1],
            &[249, 0, 0],
            &[255, 0, 0, 0, 4, 1, 3, 6, 8, 0, 9, 0, 0],
            &[0],
        ];
        assert_eq!(table, expected.concat());
    }
}
