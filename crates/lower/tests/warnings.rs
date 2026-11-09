use std::path::Path;
use syntax::config::Config;
use syntax::source::SourceFile;
use tasty::Universe;

#[test]
fn failed_primitive_conversions_warn_with_the_source_type() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let mut u = Universe::open(&jars).unwrap();
    let source = "type B = Boolean\nobject O { def f(b: B): Int = b.asInstanceOf[Int]; def g = ().asInstanceOf[Char] }";
    let source = SourceFile::new("case.scala", source);
    let (_, diags) = lower::compile_with(&mut u, &source, Config::from_options(&[]), None).unwrap();
    let messages: Vec<&str> = diags.items.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "conversion from B to Int will always fail at runtime.",
            "conversion from Unit to Char will always fail at runtime."
        ]
    );
}
