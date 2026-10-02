//! The programs `tests/programs/lowered.txt` lists, through the Rust frontend and lowering, run
//! from the NIR it writes; the output must equal `<name>.check` (research 10 §5, level 3).

use std::path::{Path, PathBuf};
use std::process::Command;
use syntax::source::SourceFile;
use tasty::Universe;

#[test]
fn lowered_programs_print_what_scala_native_prints() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let jars = typer::dump::library_jars(&root.join("ref/libs"));
    if !jars[0].exists() && std::env::var_os("CI").is_none() {
        eprintln!("skipped: the library jars are not in ref/libs");
        return;
    }
    let programs = std::fs::read_to_string(root.join("tests/programs/lowered.txt")).unwrap();
    for program in programs.lines().filter(|p| !p.is_empty()) {
        let mut u = Universe::open(&jars).unwrap();
        let dir = root.join("tests/programs").join(program);
        let file = format!("{program}.scala");
        let text = std::fs::read_to_string(dir.join(&file)).unwrap();
        let files = lower::compile(&mut u, &SourceFile::new(file, text), &[]).unwrap();
        let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("lowered").join(program);
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();
        for f in &files {
            std::fs::write(out.join(format!("{}.nir", f.name)), &f.bytes).unwrap();
        }
        let run = Command::new(env!("CARGO_BIN_EXE_scalai"))
            .args(["--cp", out.to_str().unwrap(), program])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&run.stderr);
        if stderr.contains("or the Coursier cache") && std::env::var_os("CI").is_none() {
            eprintln!("skipped: Scala Native libraries are not in the Coursier cache");
            return;
        }
        assert!(run.status.success(), "{program}: {stderr}");
        let expected = std::fs::read_to_string(dir.join(format!("{program}.check"))).unwrap();
        assert_eq!(String::from_utf8_lossy(&run.stdout), expected, "{program}");
    }
}
