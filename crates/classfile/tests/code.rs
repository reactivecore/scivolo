use classfile::{ATHROW, Code, FieldOp, GOTO, IFEQ, Invoke, NOP, Pool};

/// The bytes `emit` appends to a fresh code of `params` slots whose stack holds `stack` slots.
fn emitted(params: u16, stack: i32, emit: impl FnOnce(&mut Code, &mut Pool)) -> Vec<u8> {
    let (mut code, mut pool) = (Code::new(params), Pool::default());
    code.set_stack(stack);
    emit(&mut code, &mut pool);
    code.bytes
}

#[test]
fn jumps_reach_labels_forward_and_backward() {
    let mut code = Code::new(1);
    let (top, out) = (code.here(), code.new_label());
    code.load("I", 0);
    code.jump(IFEQ, out);
    code.iinc(0, -1).expect("a one-byte increment");
    code.jump(GOTO, top);
    assert!(!code.live());
    assert!(code.problem().is_some(), "out is not placed yet");
    code.place(out);
    assert!(code.live() && code.is_here(out));
    code.ret("V");
    assert_eq!(code.bytes, [0x1a, 0x99, 0, 9, 0x84, 0, 0xff, 0xa7, 0xff, 0xf9, 0xb1]);
    assert_eq!((code.offset(out), code.problem()), (Some(10), None));
    assert_eq!((code.max_stack, code.max_locals), (1, 1));
}

#[test]
fn switches_pad_their_operands_to_four_bytes() {
    let mut code = Code::new(1);
    let (a, b, default) = (code.new_label(), code.new_label(), code.new_label());
    code.load("I", 0);
    code.table_switch(5, default, &[a, b]);
    assert!(!code.live());
    for l in [a, b, default] {
        code.place(l);
        code.ret("V");
    }
    let table = [0x1a, 0xaa, 0, 0, 0, 0, 0, 25, 0, 0, 0, 5, 0, 0, 0, 6, 0, 0, 0, 23, 0, 0, 0, 24];
    assert_eq!(code.bytes, [&table[..], &[0xb1; 3]].concat());

    let mut code = Code::new(1);
    let (a, b, default) = (code.new_label(), code.new_label(), code.new_label());
    code.load("I", 0);
    code.load("I", 0);
    code.pop();
    code.lookup_switch(default, &[(-1, a), (10, b)]);
    for l in [a, b, default] {
        code.place(l);
        code.ret("V");
    }
    let lookup = [0, 0, 0, 27, 0, 0, 0, 2, 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 25, 0, 0, 0, 10, 0, 0, 0, 26];
    assert_eq!(code.bytes, [&[0x1a, 0x1a, 0x57, 0xab][..], &lookup, &[0xb1; 3]].concat());
}

#[test]
fn far_jumps_widen_and_move_switches_handlers_and_lines() {
    let mut code = Code::new(1);
    let (d, end) = (code.new_label(), code.new_label());
    code.load("I", 0);
    code.jump(IFEQ, end);
    code.load("I", 0);
    code.table_switch(0, d, &[d]);
    code.place(d);
    code.line(3);
    (0..33_000).for_each(|_| code.insn(NOP));
    code.place(end);
    code.ret("V");
    code.jump(GOTO, d);
    code.protect(d, end, end, 0);
    code.widen();
    let head = [0x1a, 0x9a, 0, 8, 0xc8, 0, 0, 0x81, 0x00, 0x1a, 0xaa, 0, 0, 0, 0, 18, 0, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(code.bytes[..28], [&head[..], &[0, 0, 0, 18]].concat());
    assert_eq!(code.bytes[33_028..], [0xb1, 0xc8, 0xff, 0xff, 0x7f, 0x17]);
    assert_eq!(code.handler_offsets(), Some(vec![[28, 33_028, 33_028, 0]]));
    assert_eq!(code.line_offsets().collect::<Vec<_>>(), [28]);
}

#[test]
fn a_jump_out_of_reach_after_widening_widens_too() {
    let mut code = Code::new(1);
    let (a, b) = (code.new_label(), code.new_label());
    code.jump(GOTO, b);
    code.load("I", 0);
    code.jump(IFEQ, a);
    (7..32_767).for_each(|_| code.insn(NOP));
    code.place(b);
    (32_767..33_004).for_each(|_| code.insn(NOP));
    code.place(a);
    code.ret("V");
    code.widen();
    let head = [0xc8, 0, 0, 0x80, 0x06, 0x1a, 0x9a, 0, 8, 0xc8, 0, 0, 0x80, 0xea, 0];
    assert_eq!(code.bytes[..15], head);
    assert_eq!((code.offset(b), code.offset(a)), (Some(32_774), Some(33_011)));
}

#[test]
fn iinc_takes_wide_forms_and_refuses_increments_past_two_bytes() {
    let cases: [(u16, i32, &[u8]); 6] = [
        (1, 1, &[0x84, 1, 1]),
        (1, -128, &[0x84, 1, 0x80]),
        (300, 1, &[0xc4, 0x84, 0x01, 0x2c, 0, 1]),
        (1, 200, &[0xc4, 0x84, 0, 1, 0, 0xc8]),
        (1, -32_768, &[0xc4, 0x84, 0, 1, 0x80, 0]),
        (1, 32_768, &[]),
    ];
    for (slot, by, expected) in cases {
        let mut code = Code::new(1);
        assert_eq!(code.iinc(slot, by).is_some(), !expected.is_empty(), "iinc {slot} {by}");
        assert_eq!(code.bytes, expected, "iinc {slot} {by}");
        assert_eq!(code.max_locals, if expected.is_empty() { 1 } else { slot + 1 }, "iinc {slot} {by}");
    }
}

#[test]
fn constants_take_their_shortest_form() {
    let int = |v: i32| emitted(0, 0, |c, p| c.int(p, v));
    let cases: [(i32, &[u8]); 9] = [
        (-1, &[0x02]),
        (5, &[0x08]),
        (6, &[0x10, 6]),
        (-128, &[0x10, 0x80]),
        (127, &[0x10, 0x7f]),
        (128, &[0x11, 0, 0x80]),
        (-32_768, &[0x11, 0x80, 0]),
        (32_767, &[0x11, 0x7f, 0xff]),
        (32_768, &[0x12, 1]),
    ];
    for (v, expected) in cases {
        assert_eq!(int(v), expected, "int {v}");
    }
    let wide_ldc = emitted(0, 0, |c, p| {
        (0..256).for_each(|i| _ = p.utf8(&i.to_string()));
        c.int(p, 1 << 20)
    });
    assert_eq!(wide_ldc, [0x13, 1, 1]);

    assert_eq!(emitted(0, 0, |c, p| c.long(p, 0)), [0x09]);
    assert_eq!(emitted(0, 0, |c, p| c.long(p, 1)), [0x0a]);
    assert_eq!(emitted(0, 0, |c, p| c.long(p, 2)), [0x14, 0, 1]);
    assert_eq!(emitted(0, 0, |c, p| c.float(p, 0.0)), [0x0b]);
    assert_eq!(emitted(0, 0, |c, p| c.float(p, 2.0)), [0x0d]);
    assert_eq!(emitted(0, 0, |c, p| c.float(p, -0.0)), [0x12, 1]);
    assert_eq!(emitted(0, 0, |c, p| c.float(p, f32::NAN)), [0x12, 1]);
    assert_eq!(emitted(0, 0, |c, p| c.double(p, 1.0)), [0x0f]);
    assert_eq!(emitted(0, 0, |c, p| c.double(p, -0.0)), [0x14, 0, 1]);
    assert_eq!(emitted(0, 0, |c, p| c.double(p, 2.0)), [0x14, 0, 1]);
}

#[test]
fn locals_take_short_long_and_wide_forms() {
    let cases: [(&str, bool, u16, &[u8], u16); 9] = [
        ("I", false, 3, &[0x1d], 4),
        ("Z", false, 1, &[0x1b], 2),
        ("J", false, 4, &[0x16, 4], 6),
        ("D", false, 0, &[0x26], 2),
        ("Ljava/lang/String;", false, 0, &[0x2a], 1),
        ("[I", false, 2, &[0x2c], 3),
        ("J", true, 2, &[0x41], 4),
        ("F", true, 300, &[0xc4, 0x38, 0x01, 0x2c], 301),
        ("D", true, 300, &[0xc4, 0x39, 0x01, 0x2c], 302),
    ];
    for (desc, store, slot, expected, max_locals) in cases {
        let mut code = Code::new(0);
        code.set_stack(2);
        match store {
            true => code.store(desc, slot),
            false => code.load(desc, slot),
        }
        assert_eq!((&code.bytes[..], code.max_locals), (expected, max_locals), "{desc} {slot}");
    }
}

#[test]
fn returns_elements_and_drops_follow_the_descriptor() {
    let ret = |d: &str| emitted(0, 2, |c, _| c.ret(d));
    assert_eq!(
        [ret("V"), ret("Z"), ret("J"), ret("F"), ret("D"), ret("[I")].concat(),
        [0xb1, 0xac, 0xad, 0xae, 0xaf, 0xb0]
    );
    let element = |d: &str, store| emitted(0, 4, |c, _| c.element(d, store));
    let elements = [element("Z", false), element("C", true), element("J", true), element("[I", false)];
    assert_eq!(elements.concat(), [0x33, 0x55, 0x50, 0x32]);
    let drop = |d: &str| emitted(0, 2, |c, _| c.drop_value(d));
    assert_eq!([drop("V"), drop("I"), drop("J")].concat(), [0x57, 0x58]);
}

#[test]
fn new_arrays_of_primitives_and_classes() {
    assert_eq!(emitted(0, 1, |c, p| c.new_array(p, "I")), [0xbc, 10]);
    assert_eq!(emitted(0, 1, |c, p| c.new_array(p, "Z")), [0xbc, 4]);
    let mut pool = Pool::default();
    let mut code = Code::new(0);
    code.set_stack(1);
    code.new_array(&mut pool, "Ljava/lang/String;");
    code.new_array(&mut pool, "[I");
    let [s0, s1] = pool.class("java/lang/String").to_be_bytes();
    let [a0, a1] = pool.class("[I").to_be_bytes();
    assert_eq!(code.bytes, [0xbd, s0, s1, 0xbd, a0, a1]);
}

#[test]
fn calls_and_fields_change_the_stack_by_their_descriptors() {
    let mut pool = Pool::default();
    let mut code = Code::new(0);
    code.set_stack(4);
    code.invoke(Invoke::Interface, &mut pool, "a/I", "m", "(JI)V");
    let [i0, i1] = pool.member(classfile::Ref::InterfaceMethod, "a/I", "m", "(JI)V").to_be_bytes();
    assert_eq!((&code.bytes[..], code.stack()), (&[0xb9, i0, i1, 4, 0][..], 0));
    code.invoke(Invoke::Static, &mut pool, "a/C", "f", "()J");
    assert_eq!((code.stack(), code.max_stack), (2, 4));
    code.set_stack(1);
    code.field(FieldOp::GetField, &mut pool, "a/C", "x", "D");
    assert_eq!(code.stack(), 2);
    code.set_stack(3);
    code.field(FieldOp::PutField, &mut pool, "a/C", "x", "D");
    assert_eq!(code.stack(), 0);
}

#[test]
fn set_stack_starts_a_label_after_code_that_does_not_fall_through() {
    let mut code = Code::new(0);
    let after = code.new_label();
    code.aconst_null();
    code.insn(ATHROW);
    assert_eq!((code.live(), code.stack()), (false, 0));
    code.place(after);
    code.set_stack(3);
    assert_eq!((code.live(), code.stack(), code.max_stack), (true, 3, 3));
    code.set_stack(1);
    assert_eq!(code.max_stack, 3);
}

#[test]
fn line_marks_keep_the_last_line_at_an_offset_and_drop_marks_past_the_code() {
    let mut code = Code::new(0);
    assert_eq!(code.line_table(), None);
    code.line(3);
    code.insn(NOP);
    code.line(3);
    code.line(4);
    code.line(5);
    code.insn(NOP);
    code.line(6);
    assert_eq!(code.line_offsets().collect::<Vec<_>>(), [0, 1, 2]);
    assert_eq!(code.line_table(), Some(vec![0, 2, 0, 0, 0, 3, 0, 1, 0, 5]));
    let mut end_only = Code::new(0);
    end_only.line(1);
    assert_eq!(end_only.line_table(), None);
}
