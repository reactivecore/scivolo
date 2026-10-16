//! The transcript `tests/repl/deps/circe.check`, run with Circe on the classpath; skipped when
//! Coursier cannot provide it.

mod transcript;

use repl::Session;
use std::path::Path;
use transcript::{answer, non_blank};
use vm::{Classpath, Vm};

const DEPS: [&str; 1] = ["io.circe::circe-parser::0.14.10"];

fn run() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deps: Vec<String> = DEPS.map(str::to_owned).to_vec();
    let Ok(cp) = libs::deps::fetch(&deps) else {
        eprintln!("skipped: Coursier cannot fetch Circe");
        return Ok(());
    };
    let mut jars = libs::jars()?;
    jars.extend(cp.iter().cloned());
    let image = libs::image(&jars).map_err(|e| e.to_string())?;
    let classpath = Classpath::with_image(&cp, image).map_err(|e| e.to_string())?;
    let vm = Vm::new(classpath).map_err(|e| e.to_string())?;
    let mut session = Session::new(vm, image, 80).map_err(|e| e.to_string())?;
    let text =
        std::fs::read_to_string(root.join("tests/repl/deps/circe.check")).map_err(|e| e.to_string())?;
    let expected = non_blank(text.lines().map(str::to_owned));
    let actual = non_blank(answer(&mut session, &text).into_iter());
    session.shutdown().map_err(|e| e.to_string())?;
    match actual == expected {
        true => Ok(()),
        false => Err(format!("transcript differs, actual output:\n\n{}", actual.join("\n"))),
    }
}

#[test]
fn circe_transcript_matches() {
    let run = std::thread::Builder::new().stack_size(1 << 30).spawn(run).unwrap().join().unwrap();
    match run {
        Err(e) if e.contains("or the Coursier cache") && std::env::var_os("CI").is_none() => {
            eprintln!("skipped: Scala Native libraries are not in the Coursier cache");
        }
        run => run.unwrap(),
    }
}
