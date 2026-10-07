//! A VM session: NIR added at run time instead of read from the classpath, modules evaluated,
//! values shown. The collector serves one VM per process, so one test.

#[path = "../src/libs.rs"]
mod libs;

use std::path::Path;
use vm::{Classpath, Vm, VmError};

const SOURCE: &str = "object Boom:
  val n: Int = if \"a\".length > 0 then throw new IllegalStateException(\"boom\") else 0
object Show:
  val s: String = \"a\" + 1
  def v: Any = (s, List(1))
";

/// The NIR files of `SOURCE` through the own frontend, empty without the TASTy jars.
fn compiled(root: &Path) -> Vec<lower::NirFile> {
    let tasty = typer::dump::library_jars(&root.join("ref/libs"));
    if !tasty[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped compiling: the library jars are not in ref/libs");
        return Vec::new();
    }
    let mut u = tasty::Universe::open(&tasty).unwrap();
    lower::compile(&mut u, &syntax::source::SourceFile::new("session.scala", SOURCE), &[]).unwrap()
}

fn session() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = libs::jars()?;
    let image = libs::image(&jars).map_err(|e| e.to_string())?;
    let classpath = Classpath::with_image(&[], image).map_err(|e| e.to_string())?;
    let mut vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    let dir = root.join("tests/programs/objects");
    for top in ["Cfg", "Cfg$"] {
        let data = std::fs::read(dir.join(format!("{top}.nir"))).unwrap();
        vm.add_nir(top, data).map_err(|e| e.to_string())?;
    }
    let defined = |r: Result<(), VmError>| r.is_err_and(|e| e.to_string().contains("already defined"));
    assert!(defined(vm.add_nir("Cfg$", Vec::new())));
    vm.start_runtime("session", &[]).map_err(|e| e.to_string())?;
    vm.eval_module("Cfg$").map_err(|e| e.to_string())?;
    let missing = vm.eval_module("Absent$").unwrap_err().to_string();
    assert!(missing.contains("Absent$"), "{missing}");
    assert!(defined(vm.add_nir("Absent$", Vec::new())));
    let files = compiled(&root);
    let any = !files.is_empty();
    for f in files {
        vm.add_nir(&f.name, f.bytes).map_err(|e| e.to_string())?;
    }
    if any {
        let boom = vm.eval_module("Boom$").unwrap_err().to_string();
        assert!(boom.contains("IllegalStateException: boom"), "{boom}");
        vm.eval_module("Boom$").map_err(|e| e.to_string())?;
        let shown = vm.show_value("Show$", "v", [80, 50, 0]).map_err(|e| e.to_string())?;
        assert_eq!(shown, "(\"a1\", List(1))");
    }
    vm.shutdown().map_err(|e| e.to_string())
}

#[test]
fn nir_added_at_run_time() {
    let run = std::thread::Builder::new().stack_size(1 << 30).spawn(session).unwrap().join().unwrap();
    match run {
        Err(e) if e.contains("or the Coursier cache") && std::env::var_os("CI").is_none() => {
            eprintln!("skipped: Scala Native libraries are not in the Coursier cache");
        }
        run => run.unwrap(),
    }
}
