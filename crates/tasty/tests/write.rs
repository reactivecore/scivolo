use tasty::write::{Addr, Buffer, Name, ParamSig, Pickler, TreeBuffer, pjw_hash64};
use tasty::{Cursor, Tasty};

/// `bytes` decoded and written again: every name of its table in order, its sections as they are.
fn reencode(bytes: &[u8]) -> Vec<u8> {
    let t = Tasty::parse(bytes).unwrap();
    let mut p = Pickler::default();
    for (r, _) in t.names.iter() {
        assert_eq!(p.names.index(&Name::read(&t.names, r).unwrap()), r);
    }
    for s in &t.sections {
        let id = p.new_section(&t.names.show(s.name));
        p.set_section(id, s.bytes.to_vec());
    }
    p.assemble()
}

/// The offset after the header's UUID.
fn header_len(bytes: &[u8]) -> usize {
    let mut c = Cursor::new(bytes);
    c.bytes(4).unwrap();
    (0..3).for_each(|_| _ = c.nat().unwrap());
    c.utf8().unwrap();
    c.pos() + 16
}

#[test]
fn quotes_of_unsafe_package_compat_reencode_identically() {
    for n in 1..=2 {
        let path =
            format!("{}/../../tests/pickle/unsafe-package-compat-{n}.tasty", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(path).unwrap();
        assert_eq!(reencode(&bytes), bytes, "quote {n}");
    }
}

#[test]
fn class_files_reencode_identically_after_the_tooling_version() {
    for name in ["Circle", "Main", "Rect", "Sample$package", "Shape", "Show"] {
        let path = format!("{}/tests/fixtures/{name}.tasty", env!("CARGO_MANIFEST_DIR"));
        let bytes = std::fs::read(path).unwrap();
        let ours = reencode(&bytes);
        let (h, o) = (header_len(&bytes), header_len(&ours));
        assert_eq!(ours[o - 16..], bytes[h - 16..], "{name}");
    }
}

#[test]
fn names_of_every_kind_round_trip() {
    let u = |s: &str| Box::new(Name::utf8(s));
    let names = [
        Name::Qualified(u("scala"), u("Int")),
        Name::Expanded(u("C"), u("f")),
        Name::ExpandPrefix(u("C"), u("g")),
        Name::Unique { under: Some(u("x")), sep: "$".into(), num: 3 },
        Name::Unique { under: None, sep: "_$".into(), num: 1 },
        Name::DefaultGetter(u("apply"), 2),
        Name::SuperAccessor(u("h")),
        Name::InlineAccessor(u("i")),
        Name::BodyRetainer(u("j")),
        Name::ObjectClass(u("Predef")),
        Name::Signed {
            orig: u("f"),
            target: u("f"),
            result: Box::new(Name::Qualified(u("scala"), u("Int"))),
            params: vec![ParamSig::TypeParams(2), ParamSig::Term(Name::utf8("T"))],
        },
        Name::Signed {
            orig: u("f"),
            target: u("g"),
            result: u("R"),
            params: (0..40).map(|i| ParamSig::Term(Name::utf8(&format!("P{i}")))).collect(),
        },
        Name::utf8("ünïcode"),
    ];
    let mut p = Pickler::default();
    let ast = p.new_section("ASTs");
    p.set_section(ast, vec![1, 2, 3]);
    let refs: Vec<_> = names.iter().map(|n| p.names.index(n)).collect();
    let bytes = p.assemble();
    let t = Tasty::parse(&bytes).unwrap();
    assert_eq!(t.names.len(), p.names.len());
    for (n, r) in names.iter().zip(refs) {
        assert_eq!(Name::read(&t.names, r).as_ref(), Some(n));
    }
    assert_eq!(t.section("ASTs").unwrap().bytes, [1, 2, 3]);
    assert_eq!(t.header.tooling, "Scala 3.9.0");
}

#[test]
fn numbers_round_trip() {
    let nats = [0, 1, 127, 128, 16383, 16384, u32::MAX as u64, u64::MAX >> 1];
    let ints = [0, 1, -1, 63, 64, -64, -65, i64::MAX, i64::MIN, 1 << 62];
    let mut b = Buffer::default();
    nats.iter().for_each(|&n| b.write_nat(n));
    ints.iter().for_each(|&n| b.write_long_int(n));
    let mut c = Cursor::new(&b.bytes);
    assert_eq!(nats.map(|_| c.nat().unwrap()), nats);
    assert_eq!(ints.map(|_| c.long_int().unwrap()), ints);
    assert!(c.at_end());
}

#[test]
fn pjw_hash64_matches_dotty() {
    assert_eq!(pjw_hash64(b""), 0);
    assert_eq!(pjw_hash64(b"abc"), 0x616263);
    assert_eq!(pjw_hash64(&[0xff; 9]), 0x00ff_ffff_ff00_00ff);
}

/// Nodes `[tag, length, ref to an earlier node, payload]`: after `compactify` each length still
/// spans its node and each ref points at its target's new address.
#[test]
fn compactify_shrinks_refs_and_keeps_them_valid() {
    let mut t = TreeBuffer::default();
    let mut nodes = Vec::new();
    for i in 0..200u32 {
        let start = t.register_tree(i);
        t.buf.write_byte(i as u8 | 0x80);
        let len = t.reserve_ref(true);
        let target = t.addr_of_tree(i / 3).unwrap();
        t.write_ref(target);
        t.buf.write_bytes(&vec![7; i as usize % 5]);
        let end = t.buf.current_addr();
        t.fill_ref(len, end, true);
        nodes.push((start, i / 3, i as usize % 5));
    }
    let before = t.buf.len();
    t.compactify();
    assert!(t.buf.len() < before - 200 * 4);
    let mut c = Cursor::new(&t.buf.bytes);
    for (i, &(_, target, payload)) in nodes.iter().enumerate() {
        assert_eq!(Some(Addr(c.pos())), t.addr_of_tree(i as u32));
        assert_eq!(c.byte().unwrap(), i as u8 | 0x80);
        let end = c.end().unwrap();
        assert_eq!(Some(Addr(c.nat().unwrap() as usize)), t.addr_of_tree(target));
        assert_eq!(c.bytes(payload).unwrap(), vec![7; payload]);
        assert_eq!(c.pos(), end);
    }
    assert!(c.at_end());
}
