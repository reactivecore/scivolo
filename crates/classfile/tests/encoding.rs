use classfile::{Pool, method_slots, split_method};

#[test]
fn method_descriptors_split_into_parameters() {
    let (params, result) = split_method("(I[[Ljava/lang/String;JLscala/Unit;)D");
    assert_eq!(params, ["I", "[[Ljava/lang/String;", "J", "Lscala/Unit;"]);
    assert_eq!(result, "D");
    assert_eq!(method_slots("(I[[Ljava/lang/String;JLscala/Unit;)D"), (5, 2));
    assert_eq!(method_slots("()V"), (0, 0));
}

#[test]
fn strings_are_modified_utf8_and_added_once() {
    let mut pool = Pool::default();
    let text: Vec<u16> = "a\0é😀".encode_utf16().collect();
    assert_eq!(pool.string(&text), pool.string(&text));
    let mut out = Vec::new();
    pool.write(&mut out).expect("a small pool");
    let utf8 = [0x61, 0xc0, 0x80, 0xc3, 0xa9, 0xed, 0xa0, 0xbd, 0xed, 0xb8, 0x80];
    let expected: Vec<u8> = [&[0, 3, 1, 0, 11][..], &utf8, &[8, 0, 1]].concat();
    assert_eq!(out, expected);
}
