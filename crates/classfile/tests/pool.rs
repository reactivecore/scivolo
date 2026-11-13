use classfile::{Constant, Handle, Pool, Ref};

#[test]
fn entries_are_added_once_and_longs_take_two_indices() {
    let mut pool = Pool::default();
    assert_eq!((pool.utf8("a"), pool.class("a"), pool.class("a")), (1, 2, 2));
    assert_eq!((pool.long(5), pool.integer(-1), pool.long(5)), (3, 5, 3));
    assert_eq!((pool.double(1.0), pool.float(1.0)), (6, 8));
    assert_ne!(pool.float(0.0), pool.float(-0.0));
    let method = pool.member(Ref::Method, "a", "m", "()V");
    assert_ne!(method, pool.member(Ref::InterfaceMethod, "a", "m", "()V"));
    assert_eq!(pool.member_parts(method), ("a".into(), "m".into(), "()V".into()));

    let mut pool = Pool::default();
    pool.class("a");
    pool.long(5);
    pool.integer(-1);
    let mut out = Vec::new();
    pool.write(&mut out).expect("a small pool");
    let long = [5, 0, 0, 0, 0, 0, 0, 0, 5];
    let expected = [&[0, 6, 1, 0, 1, b'a', 7, 0, 1][..], &long, &[3, 0xff, 0xff, 0xff, 0xff]].concat();
    assert_eq!(out, expected);
}

#[test]
fn a_pool_holds_at_most_65534_entries() {
    let mut pool = Pool::default();
    (0..65_534).for_each(|i| _ = pool.integer(i));
    assert!(pool.write(&mut Vec::new()).is_some());
    pool.integer(-1);
    assert!(pool.write(&mut Vec::new()).is_none());

    let mut pool = Pool::default();
    (0..65_533).for_each(|i| _ = pool.integer(i));
    assert_eq!(pool.long(-1), 65_534);
    assert!(pool.write(&mut Vec::new()).is_none(), "the long's second slot is past the pool");
}

#[test]
fn call_sites_share_their_bootstrap_methods() {
    let mut pool = Pool::default();
    assert_eq!(pool.bootstrap_methods(), None);
    let meta = "(Ljava/lang/invoke/MethodHandles$Lookup;Ljava/lang/String;)Ljava/lang/invoke/CallSite;";
    let boot = Handle::static_method("a/B", "boot", meta);
    let args = [Constant::Int(3), Constant::MethodType("()V")];
    let apply = pool.invoke_dynamic("apply", "()La/F;", &boot, &args);
    assert_eq!(apply, 15);
    assert_eq!(pool.invoke_dynamic("other", "()La/F;", &boot, &args), 18);
    assert_eq!(pool.invoke_dynamic("apply", "()La/F;", &boot, &args), apply);
    assert_eq!(pool.invoke_dynamic("apply", "()La/F;", &boot, &[Constant::Int(4)]), 20);

    let (name, bytes) = pool.bootstrap_methods().expect("call sites");
    assert_eq!((name, pool.find_utf8("BootstrapMethods")), (11, Some(11)));
    assert_eq!(bytes, [0, 2, 0, 7, 0, 2, 0, 8, 0, 10, 0, 7, 0, 1, 0, 19]);
    assert_eq!(pool.member_parts(apply), (String::new(), "apply".into(), "()La/F;".into()));
    let descriptors = [7, 10, 2, 8, apply].map(|i| pool.constant_descriptor(i));
    let expected =
        ["Ljava/lang/invoke/MethodHandle;", "Ljava/lang/invoke/MethodType;", "Ljava/lang/Class;", "I", "V"];
    assert_eq!(descriptors, expected);

    let mut out = Vec::new();
    pool.write(&mut out).expect("a small pool");
    let at = |s: &[u8]| out.windows(s.len()).any(|w| w == s);
    assert!(at(&[15, 6, 0, 6]), "the handle is REF_invokeStatic of Methodref 6");
    assert!(at(&[16, 0, 9]) && at(&[18, 0, 0, 0, 14]) && at(&[18, 0, 1, 0, 14]));
}
