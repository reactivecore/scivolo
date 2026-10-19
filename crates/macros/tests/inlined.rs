//! The macro calls of the inputs in `tests/inlined` expanded in the VM, against the `Inlined`
//! trees of dotty 3.9.0's `inlining` phase recorded next to each in a `.check` file
//! (`just typer-checks`), which ran the same library's class files on the JVM.

use macros::VmMacros;
use std::path::{Path, PathBuf};
use syntax::source::SourceFile;
use tasty::Universe;
use typer::dump::{dump_with, library_jars};
use vm::{Classpath, Vm};

fn run() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lib = root.join("tests/macros/lib/macros.jar");
    let jars = libs::jars()?;
    let image = libs::image(&jars).map_err(|e| e.to_string())?;
    let classpath = Classpath::with_image(std::slice::from_ref(&lib), image).map_err(|e| e.to_string())?;
    let mut vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    let mut tasty = library_jars(&root.join("ref/libs"));
    tasty.push(lib);
    let mut inputs: Vec<PathBuf> =
        std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/inlined"))
            .map_err(|e| e.to_string())?
            .map(|e| e.expect("an entry").path())
            .filter(|p| p.extension().is_some_and(|e| e == "scala"))
            .collect();
    inputs.sort();
    for input in inputs {
        let expected = std::fs::read_to_string(input.with_extension("check")).expect("a .check file");
        let text = std::fs::read_to_string(&input).expect("a test input");
        let mut u = Universe::open(&tasty).map_err(|e| e.to_string())?;
        let source = SourceFile::new(input.to_string_lossy(), text.as_str());
        let mut runner = VmMacros { vm: &mut vm };
        let actual = dump_with(&mut u, &source, &[], (false, true), Some(&mut runner));
        if actual != expected {
            let at = actual.bytes().zip(expected.bytes()).take_while(|(a, b)| a == b).count();
            let around =
                |s: &str| s.get(at.saturating_sub(300)..(at + 200).min(s.len())).unwrap_or("").to_owned();
            panic!(
                "{}: differs at {at}\n  dotty: {}\n  ours:  {}",
                input.display(),
                around(&expected),
                around(&actual)
            );
        }
    }
    Ok(())
}

#[test]
fn expansions_match_dotty() {
    let run = std::thread::Builder::new().stack_size(1 << 30).spawn(run).unwrap().join().unwrap();
    match run {
        Err(e) if e.contains("or the Coursier cache") && std::env::var_os("CI").is_none() => {
            eprintln!("skipped: Scala Native libraries are not in the Coursier cache");
        }
        run => run.unwrap(),
    }
}
