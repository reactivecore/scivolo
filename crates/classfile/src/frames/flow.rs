//! Instruction lengths, the targets of jumps and switches, and the exception table without dead
//! code.

/// The length of the instruction at `at`.
pub fn length(code: &[u8], at: usize) -> usize {
    match code[at] {
        0x10 | 0x12 | 0x15..=0x19 | 0x36..=0x3a | 0xbc => 2,
        0x11 | 0x13 | 0x14 | 0x84 | 0x99..=0xa7 | 0xb2..=0xb8 | 0xbb | 0xbd | 0xc0 | 0xc1 | 0xc6 | 0xc7 => 3,
        0xb9 | 0xba | 0xc8 => 5,
        0xc4 if code[at + 1] == 0x84 => 6,
        0xc4 => 4,
        0xaa | 0xab => {
            let base = operands(at);
            base + 4 * switch_slots(code, at) - at
        }
        _ => 1,
    }
}

/// The targets of the jump or switch at `at`, none for another instruction.
pub fn targets(code: &[u8], at: usize) -> Vec<usize> {
    let to = |offset: i64| (at as i64 + offset) as usize;
    match code[at] {
        0x99..=0xa7 | 0xc6 | 0xc7 => vec![to(i16::from_be_bytes([code[at + 1], code[at + 2]]).into())],
        0xc8 => vec![to(int(code, at + 1).into())],
        0xaa | 0xab => {
            let (base, slots) = (operands(at), switch_slots(code, at));
            let firsts = if code[at] == 0xaa { 3..slots } else { 2..slots };
            let step = if code[at] == 0xaa { 1 } else { 2 };
            let at_slot = |i: usize| to(int(code, base + 4 * i).into());
            std::iter::once(0).chain(firsts.skip(step - 1).step_by(step)).map(at_slot).collect()
        }
        _ => Vec::new(),
    }
}

/// The offset of the 4-byte aligned operands of the switch at `at`.
fn operands(at: usize) -> usize {
    (at + 4) & !3
}

/// The 4-byte operands of the switch at `at`.
fn switch_slots(code: &[u8], at: usize) -> usize {
    let base = operands(at);
    match code[at] {
        0xaa => 3 + (i64::from(int(code, base + 8)) - i64::from(int(code, base + 4)) + 1) as usize,
        _ => 2 + 2 * int(code, base + 4) as usize,
    }
}

fn int(code: &[u8], i: usize) -> i32 {
    i32::from_be_bytes([code[i], code[i + 1], code[i + 2], code[i + 3]])
}

/// The exception table of protected ranges `handlers` (start, end, handler, class index) without
/// the dead code ranges `dead` (ASM's `Handler.removeRange`).
pub fn live_ranges(handlers: &[[usize; 4]], dead: &[(usize, usize)]) -> Vec<[u16; 4]> {
    let mut out = Vec::new();
    for &[start, end, handler, catch] in handlers {
        let mut pieces = vec![(start, end)];
        for &(ds, de) in dead {
            let split = |(s, e): (usize, usize)| [(s, e.min(ds)), (s.max(de), e)];
            pieces = pieces.into_iter().flat_map(split).filter(|(s, e)| s < e).collect();
        }
        out.extend(pieces.into_iter().map(|(s, e)| [s as u16, e as u16, handler as u16, catch as u16]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{length, live_ranges, targets};

    fn ints(values: &[i32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_be_bytes()).collect()
    }

    #[test]
    fn switches_span_their_padding_and_jump_tables() {
        let table = [&[0x1a, 0xaa, 0, 0][..], &ints(&[30, 5, 6, 40, 50])].concat();
        assert_eq!((length(&table, 1), targets(&table, 1)), (23, vec![31, 41, 51]));
        let lookup = [&[0, 0, 0, 0xab][..], &ints(&[27, 2, -1, 25, 10, 26])].concat();
        assert_eq!((length(&lookup, 3), targets(&lookup, 3)), (25, vec![30, 28, 29]));
        let empty = [&[0xab, 0, 0, 0][..], &ints(&[8, 0])].concat();
        assert_eq!((length(&empty, 0), targets(&empty, 0)), (12, vec![8]));
    }

    #[test]
    fn instructions_have_their_lengths_and_targets() {
        let cases: [(&[u8], usize, usize, &[usize]); 11] = [
            (&[0x60], 0, 1, &[]),
            (&[0x10, 1], 0, 2, &[]),
            (&[0xbc, 10], 0, 2, &[]),
            (&[0x11, 1, 0], 0, 3, &[]),
            (&[0x84, 1, 1], 0, 3, &[]),
            (&[0xb9, 0, 1, 2, 0], 0, 5, &[]),
            (&[0xc4, 0x15, 1, 0], 0, 4, &[]),
            (&[0xc4, 0x84, 1, 0, 0, 1], 0, 6, &[]),
            (&[0, 0, 0xa7, 0xff, 0xfe], 2, 3, &[0]),
            (&[0xc7, 0, 6], 0, 3, &[6]),
            (&[0, 0xc8, 0xff, 0xff, 0xff, 0xff], 1, 5, &[0]),
        ];
        for (code, at, len, to) in cases {
            assert_eq!((length(code, at), &targets(code, at)[..]), (len, to), "{code:x?}");
        }
    }

    #[test]
    fn dead_code_leaves_the_protected_ranges() {
        let cases = [
            (vec![], vec![[0, 20, 30, 7]]),
            (vec![(5, 10)], vec![[0, 5, 30, 7], [10, 20, 30, 7]]),
            (vec![(0, 5), (15, 25)], vec![[5, 15, 30, 7]]),
            (vec![(0, 20)], vec![]),
            (vec![(20, 30)], vec![[0, 20, 30, 7]]),
        ];
        for (dead, expected) in cases {
            assert_eq!(live_ranges(&[[0, 20, 30, 7]], &dead), expected, "{dead:?}");
        }
    }
}
