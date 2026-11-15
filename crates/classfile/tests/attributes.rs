use classfile::{ACC_FINAL, ACC_PUBLIC, ACC_STATIC, ClassFile, Code, Handle, InnerClass};

/// A class with a source file, inner classes, custom attributes and a method with parameters and
/// a call site.
fn class() -> Vec<u8> {
    let mut cf = ClassFile::new(ACC_PUBLIC, "a/C", "java/lang/Object", &[]);
    cf.custom_attribute("Scala", Vec::new());
    cf.source_file("C.scala");
    let nested = InnerClass {
        inner: "a/C$D".into(),
        outer: Some("a/C".into()),
        name: Some("D".into()),
        flags: ACC_STATIC,
    };
    let anonymous = InnerClass { inner: "a/C$1".into(), outer: None, name: None, flags: 0 };
    cf.inner_classes(&[nested, anonymous]);
    cf.custom_attribute("TASTY", vec![1, 2]);
    let boot = Handle::static_method("a/B", "boot", "()Ljava/lang/invoke/CallSite;");
    let mut code = Code::new(0);
    code.invoke_dynamic(&mut cf.pool, ("run", "()Ljava/lang/Runnable;"), &boot, &[]);
    code.pop();
    code.ret("V");
    cf.method(ACC_STATIC, "m", "()V", Some(code));
    cf.method_parameters(&[("x".into(), ACC_FINAL)]);
    cf.bytes(&mut |_, _| None).expect("a class file")
}

/// The constant pool in ASM's order: the class, the entries of its attributes, the method's
/// name, descriptor and parameters, its code, then the attributes' names in the order they are
/// written in.
#[test]
fn pool_and_attributes_follow_asm_order() {
    let bytes = class();
    let utf8 = |s: &str| [&[1, 0, s.len() as u8][..], s.as_bytes()].concat();
    let pool = [
        &[0, 32][..],
        &utf8("a/C"),
        &[7, 0, 1],
        &utf8("java/lang/Object"),
        &[7, 0, 3],
        &utf8("C.scala"),
        &utf8("a/C$D"),
        &[7, 0, 6],
        &utf8("D"),
        &utf8("a/C$1"),
        &[7, 0, 9],
        &utf8("m"),
        &utf8("()V"),
        &utf8("x"),
        &utf8("a/B"),
        &[7, 0, 14],
        &utf8("boot"),
        &utf8("()Ljava/lang/invoke/CallSite;"),
        &[12, 0, 16, 0, 17, 10, 0, 15, 0, 18, 15, 6, 0, 19],
        &utf8("run"),
        &utf8("()Ljava/lang/Runnable;"),
        &[12, 0, 21, 0, 22, 18, 0, 0, 0, 23],
        &utf8("Code"),
        &utf8("MethodParameters"),
        &utf8("InnerClasses"),
        &utf8("SourceFile"),
        &utf8("BootstrapMethods"),
        &utf8("TASTY"),
        &utf8("Scala"),
    ];
    assert!(bytes[8..].starts_with(&pool.concat()));

    let parameters = [0, 26, 0, 0, 0, 5, 1, 0, 13, 0, 0x10];
    assert!(bytes.windows(parameters.len()).any(|w| w == parameters));
    let tail = [
        &[0, 5][..],
        &[0, 27, 0, 0, 0, 18, 0, 2, 0, 7, 0, 2, 0, 8, 0, 8, 0, 10, 0, 0, 0, 0, 0, 0],
        &[0, 28, 0, 0, 0, 2, 0, 5],
        &[0, 29, 0, 0, 0, 6, 0, 1, 0, 20, 0, 0],
        &[0, 30, 0, 0, 0, 2, 1, 2],
        &[0, 31, 0, 0, 0, 0],
    ];
    assert!(bytes.ends_with(&tail.concat()));
}
