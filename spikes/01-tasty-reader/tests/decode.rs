use tasty_reader::{Ast, Cursor, Tasty, dump};

#[test]
fn nat_and_long_int_use_stop_bit_on_last_byte() {
    let mut c = Cursor::new(&[0x81, 0x01, 0x80, 0xff, 0x7f, 0xff]);
    assert_eq!(c.nat().unwrap(), 1);
    assert_eq!(c.nat().unwrap(), 128);
    assert_eq!(c.long_int().unwrap(), -1);
    assert_eq!(c.long_int().unwrap(), -1);
}

fn sample(name: &str) -> String {
    let path = format!("{}/artifacts/{name}.tasty", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(path).unwrap();
    let t = Tasty::parse(&bytes).unwrap();
    let ast = Ast::parse(t.section("ASTs").unwrap().bytes).unwrap();
    assert_eq!(ast.dangling_refs(), 0);
    dump(&ast, &t.names)
}

#[test]
fn sample_files_decode() {
    for f in ["Circle", "Rect", "Shape", "Show", "Sample$package"] {
        sample(f);
    }
    let main = sample("Main");
    assert!(main.contains("APPLY\n              sample.Sample$package.twice"));
    assert!(main.contains("INLINED 1 Sample$package$"));
}
