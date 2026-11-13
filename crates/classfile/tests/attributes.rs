use classfile::{ACC_FINAL, ACC_PUBLIC, ACC_STATIC, ClassFile, Code, Handle, InnerClass};

#[test]
fn class_attributes_end_with_bootstrap_methods_and_then_custom_ones() {
    let mut cf = ClassFile::new(ACC_PUBLIC, "a/C", "java/lang/Object", &[]);
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
    cf.method(ACC_STATIC, "m", "()V", Some(code), &mut |_, _| None);
    cf.method_parameters(&[("x".into(), ACC_FINAL)]);
    let bytes = cf.bytes().expect("a class file");

    let name = |cf: &ClassFile, s: &str| cf.pool.find_utf8(s).expect("a name").to_be_bytes();
    let [n0, n1] = name(&cf, "MethodParameters");
    let [x0, x1] = name(&cf, "x");
    let parameters = [n0, n1, 0, 0, 0, 5, 1, x0, x1, 0, 0x10];
    assert!(bytes.windows(parameters.len()).any(|w| w == parameters));

    let [d0, d1] = cf.pool.class("a/C$D").to_be_bytes();
    let [c0, c1] = cf.pool.class("a/C").to_be_bytes();
    let [a0, a1] = cf.pool.class("a/C$1").to_be_bytes();
    let [h0, h1] = cf.pool.handle(&boot).to_be_bytes();
    let tail = [
        &[0, 4][..],
        &name(&cf, "SourceFile"),
        &[0, 0, 0, 2],
        &name(&cf, "C.scala"),
        &name(&cf, "InnerClasses"),
        &[0, 0, 0, 18, 0, 2, d0, d1, c0, c1],
        &name(&cf, "D"),
        &[0, 8, a0, a1, 0, 0, 0, 0, 0, 0],
        &name(&cf, "BootstrapMethods"),
        &[0, 0, 0, 6, 0, 1, h0, h1, 0, 0],
        &name(&cf, "TASTY"),
        &[0, 0, 0, 2, 1, 2],
    ];
    assert!(bytes.ends_with(&tail.concat()));
}
