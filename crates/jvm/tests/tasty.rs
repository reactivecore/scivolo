//! The jar of tests/jvm/tastylib: its manifest, the TASTy of its top-level classes that their
//! `TASTY` attributes name, and scala-cli compiling tests/jvm/tastylib/user.scala against it.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Compiles tests/jvm/tastylib into the jar tmp/jvm-test/tastylib/geo.jar.
fn geo_jar() -> PathBuf {
    let out = root().join("tmp/jvm-test/tastylib");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).expect("tmp/jvm-test/tastylib");
    let jar = out.join("geo.jar");
    let source = root().join("tests/jvm/tastylib/tastylib.scala");
    let run = Command::new(env!("CARGO_BIN_EXE_scivolo-jvm")).arg("-d").arg(&jar).arg(source).output();
    let run = run.expect("running scivolo-jvm");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    jar
}

fn entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    zip.by_name(name).unwrap_or_else(|_| panic!("{name} in the jar")).read_to_end(&mut bytes).unwrap();
    bytes
}

#[test]
fn the_jar_holds_a_manifest_and_the_tasty_its_classes_name() {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(geo_jar()).unwrap()).unwrap();
    let manifest = String::from_utf8(entry(&mut zip, "META-INF/MANIFEST.MF")).unwrap();
    assert_eq!(
        manifest,
        "Manifest-Version: 1.0\r\nScala-Compiler-Version: 3.9.0\r\nMain-Class: geo.Shapes\r\n\r\n"
    );
    for (class, tasty) in [("Shapes", "Shapes"), ("Rect", "Rect"), ("Shape", "Shape"), ("Cell", "Cell")] {
        let tasty = entry(&mut zip, &format!("geo/{tasty}.tasty"));
        let uuid = tasty::Tasty::parse(&tasty).expect("our TASTy").header.uuid;
        let class = entry(&mut zip, &format!("geo/{class}.class"));
        let attribute = [&[0, 0, 0, 16][..], &uuid].concat();
        assert!(class.windows(attribute.len()).any(|w| w == attribute), "the TASTY attribute of {tasty:?}");
    }
    assert!(zip.by_name("geo/Shapes$.tasty").is_err() && zip.by_name("geo/Rect$.tasty").is_err());
}

/// scala-cli 3.9.0 compiles tests/jvm/tastylib/user.scala against the jar, and the result runs
/// on `java -Xverify:all`, when both are on the PATH.
#[test]
fn scala_cli_compiles_against_the_jar() {
    let missing = |tool: &str| Command::new(tool).arg("--version").output().is_err();
    if missing("scala-cli") || missing("java") {
        eprintln!("no scala-cli or java on the PATH: skipped");
        return;
    }
    let jar = geo_jar();
    let dir = jar.parent().expect("a directory").to_owned();
    let compiled = Command::new("scala-cli")
        .args(["--power", "compile", "-S", "3.9.0", "--server=false", "--quiet", "--workspace"])
        .arg(dir.join("ws"))
        .arg("-d")
        .arg(dir.join("user"))
        .arg("--jar")
        .arg(&jar)
        .arg(root().join("tests/jvm/tastylib/user.scala"))
        .output()
        .expect("running scala-cli");
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let library = root().join("ref/libs/scala-library-3.9.0.jar");
    let classpath = format!("{}:{}:{}", dir.join("user").display(), jar.display(), library.display());
    let run = Command::new("java").args(["-Xverify:all", "-cp", &classpath, "User"]).output().expect("java");
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let check = std::fs::read_to_string(root().join("tests/jvm/tastylib/user.check")).expect("user.check");
    assert_eq!(String::from_utf8_lossy(&run.stdout), check);
}
