use nir::{Caches, DefnKind, Pool, Reader, show};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn decodes_every_fixture() {
    let (mut pool, mut caches) = (Pool::default(), Caches::default());
    for name in ["hello.nir", "hello$package.nir", "hello$package$.nir", "Employee.nir"] {
        let data = fixture(name);
        let defns = Reader::new(&data, &mut caches, &mut pool).and_then(|mut r| r.all()).unwrap();
        assert!(!defns.is_empty(), "{name}");
    }
}

#[test]
fn shows_hello_like_scala_native() {
    let (mut pool, mut caches) = (Pool::default(), Caches::default());
    let data = fixture("hello$package$.nir");
    let defns = Reader::new(&data, &mut caches, &mut pool).and_then(|mut r| r.all()).unwrap();
    let hello = defns
        .iter()
        .find(|d| show::global(&pool, d.name, true).ends_with("::hello(): unit"))
        .expect("hello() defined");
    assert!(matches!(hello.kind, DefnKind::Define { .. }));
    let expected = "\
def @hello$package$::hello(): unit : (@hello$package$) => unit {
%2(%1 <this> : @hello$package$):
  %3 = module @scala.Predef$
  %4 = method %3 : !?@scala.Predef$, println(java.lang.Object): unit
  %5 = call[(@scala.Predef$, @java.lang.Object) => unit] %4 : ptr(%3 : !?@scala.Predef$, \"Hello, world!\")
  ret unit
}";
    assert_eq!(show::defn(&pool, hello, true), expected);
}

#[test]
fn rejects_truncated_input() {
    let (mut pool, mut caches) = (Pool::default(), Caches::default());
    let data = fixture("Employee.nir");
    for len in [0, 10, 44, data.len() / 2] {
        let result = Reader::new(&data[..len], &mut caches, &mut pool).and_then(|mut r| r.all());
        assert!(result.is_err(), "len {len}");
    }
}
