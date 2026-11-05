use classfile::{ACC_STATIC, ClassFile, Code, Pool, method_slots, split_method};

/// A static method `(ZLa/A;La/B;)Ljava/lang/Object;` choosing its second or third parameter,
/// followed by a dead `return`, with `lub` for the frames.
fn choice(lub: &mut classfile::Lub) -> Result<Vec<u8>, String> {
    let mut cf = ClassFile::new(0, "C", "java/lang/Object", &[]);
    let mut code = Code::new(3);
    let (other, end) = (code.new_label(), code.new_label());
    code.load("I", 0);
    code.jump(0x99, other);
    code.load("L", 1);
    code.jump(0xa7, end);
    code.ret("V");
    code.place(other);
    code.load("L", 2);
    code.place(end);
    code.ret("L");
    cf.method(ACC_STATIC, "choice", "(ZLa/A;La/B;)Ljava/lang/Object;", Some(code), lub);
    cf.bytes()
}

#[test]
fn frames_merge_by_the_common_superclass_and_replace_dead_code() {
    let bytes = choice(&mut |_, _| Some("a/Base".into())).expect("a class file");
    let at = |s: &[u8]| bytes.windows(s.len()).position(|w| w == s);
    assert!(at(b"StackMapTable").is_some());
    let code = [0x1a, 0x99, 0, 8, 0x2b, 0xa7, 0, 5, 0xbf, 0x2c, 0xb0];
    assert!(at(&code).is_some(), "the dead return becomes athrow");
    assert!(choice(&mut |_, _| None).unwrap_err().contains("no common superclass of a/A and a/B"));
}

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
