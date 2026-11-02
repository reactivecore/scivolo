//! A unit with `import scala.language.\`2.13\`` behaves as the Scala 2 standard library.

use std::path::Path;
use syntax::source::SourceFile;
use tasty::Universe;

#[test]
fn scala2_units_init_traits_and_keep_case_classes_scala2_shaped() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let text = "package p\nimport scala.language.`2.13`\ntrait T { def f: Int = 1 }\ncase class C(a: Int)\n";
    let mut u = Universe::open(&jars).unwrap();
    let files = lower::compile(&mut u, &SourceFile::new("t.scala", text), &[]).expect("lowered");
    let defs = |name: &str| {
        let file = files.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("{name} missing"));
        let (mut caches, mut pool) = (nir::Caches::default(), nir::Pool::default());
        let defs = nir::Reader::new(&file.bytes, &mut caches, &mut pool).unwrap().all().unwrap();
        defs.iter().map(|d| nir::show::defn_with_debug_info(&pool, d)).collect::<Vec<_>>().join("\n")
    };
    assert!(defs("p.T").contains("def @p.T::$init$()"), "{}", defs("p.T"));
    assert!(!defs("p.C").contains("::_1()"), "{}", defs("p.C"));
    let companion = defs("p.C$");
    assert!(
        companion.contains("unapply(p.C): scala.Option") && !companion.contains("fromProduct"),
        "{companion}"
    );
}
