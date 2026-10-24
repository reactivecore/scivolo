use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Every compile and run stops after this long.
const TIMEOUT: Duration = Duration::from_secs(20);

/// A compile running macros in a fresh cache stops after this long.
const MACRO_TIMEOUT: Duration = Duration::from_secs(300);

fn program(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/programs").join(name)
}

/// All of `stream`, read on a thread of its own.
fn drain(mut stream: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stream.read_to_end(&mut bytes);
        bytes
    })
}

/// The output of `cmd`, which fails the test with `what` when it runs longer than `limit`.
fn output(cmd: &mut Command, what: &str, limit: Duration) -> Output {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect(what);
    let stdout = drain(child.stdout.take().expect("piped stdout"));
    let stderr = drain(child.stderr.take().expect("piped stderr"));
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect(what) {
            break status;
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            panic!("timeout ({what})");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Output { status, stdout: stdout.join().expect(what), stderr: stderr.join().expect(what) }
}

/// What the executable `scivolo-aot` builds with `args` into `bin` prints when run with `run`.
fn build_and_run(args: &[&str], bin: &Path, run: &[&str]) -> String {
    build_with(Command::new(env!("CARGO_BIN_EXE_scivolo-aot")).args(args), bin, TIMEOUT);
    run_bin(bin, run)
}

/// Builds the executable `bin` with `compile`, stopping it after `limit`.
fn build_with(compile: &mut Command, bin: &Path, limit: Duration) {
    let out = output(compile.arg("-o").arg(bin), "compile", limit);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

/// What the executable `bin` prints when run with `run`.
fn run_bin(bin: &Path, run: &[&str]) -> String {
    let out = output(Command::new(bin).args(run), "run", TIMEOUT);
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Checks the output of the executable built from the sources in `tests/aot/name` with `args`,
/// compiled in a cache of its own without compiled sources, so that their macro calls expand.
fn check_expanded(name: &str, args: &[&str]) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/aot").join(name);
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("aot-macros").join(name);
    let cache = tmp.join("cache");
    let _ = std::fs::remove_dir_all(cache.join("aot-units"));
    let mut compile = Command::new(env!("CARGO_BIN_EXE_scivolo-aot"));
    compile.env("SCIVOLO_CACHE", &cache).args(args).arg(&dir);
    build_with(&mut compile, &tmp.join(name), MACRO_TIMEOUT);
    let out = run_bin(&tmp.join(name), &[]);
    assert_eq!(out, std::fs::read_to_string(dir.join(format!("{name}.check"))).expect("the .check file"));
}

#[test]
fn macro_calls_of_a_library_jar_expand() {
    let jar = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/macros/lib/macros.jar");
    check_expanded("macros", &["--cp", &jar.to_string_lossy()]);
}

#[test]
fn circe_codec_derives_through_its_macros() {
    let deps = ["io.circe:circe-parser_native0.5_3:0.14.10", "io.circe:circe-generic_native0.5_3:0.14.10"];
    if !in_coursier_cache(&deps) && std::env::var_os("CI").is_none() {
        eprintln!("skipped: Circe 0.14.10 is not in the Coursier cache");
        return;
    }
    check_expanded("circe", &[]);
}

#[test]
fn ujson_and_pprint_run() {
    let deps = ["com.lihaoyi:ujson_native0.5_3:4.4.3", "com.lihaoyi:pprint_native0.5_3:0.9.6"];
    if !in_coursier_cache(&deps) && std::env::var_os("CI").is_none() {
        eprintln!("skipped: ujson 4.4.3 or pprint 0.9.6 is not in the Coursier cache");
        return;
    }
    check_expanded("ujson", &[]);
    check_expanded("pprint", &[]);
    check_expanded("Lh24pprint", &[]);
}

#[test]
fn upickle_derives_through_its_macros() {
    if !in_coursier_cache(&["com.lihaoyi:upickle_native0.5_3:4.4.3"]) && std::env::var_os("CI").is_none() {
        eprintln!("skipped: upickle 4.4.3 is not in the Coursier cache");
        return;
    }
    check_expanded("Lh24upickle", &[]);
}

#[test]
fn oslib_file_operations_run() {
    if !in_coursier_cache(&["com.lihaoyi:os-lib_native0.5_3:0.11.9-M8"]) && std::env::var_os("CI").is_none() {
        eprintln!("skipped: os-lib 0.11.9-M8 is not in the Coursier cache");
        return;
    }
    check_expanded("oslib", &[]);
    check_expanded("Lh24oslib", &[]);
}

#[test]
fn sourcecode_macros_expand() {
    if !in_coursier_cache(&["com.lihaoyi:sourcecode_native0.5_3:0.4.4"]) && std::env::var_os("CI").is_none() {
        eprintln!("skipped: sourcecode 0.4.4 is not in the Coursier cache");
        return;
    }
    check_expanded("sourcecode", &[]);
}

/// Whether Coursier finds `coordinates` without going online.
fn in_coursier_cache(coordinates: &[&str]) -> bool {
    let chosen = std::env::var_os("SCIVOLO_COURSIER").map(|c| vec![c]);
    chosen.unwrap_or_else(|| vec!["coursier".into(), "cs".into()]).into_iter().any(|cs| {
        Command::new(cs)
            .args(["fetch", "--mode", "offline"])
            .args(coordinates)
            .args(["--exclude", "org.scala-lang:*", "--exclude", "org.scala-native:*"])
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// What the executable built from method `entry` of test program `name` prints for `arg`.
fn run(name: &str, entry: &str, arg: &str) -> String {
    let bin = Path::new(env!("CARGO_TARGET_TMPDIR")).join("aot").join(name);
    let cp = program(name).to_string_lossy().into_owned();
    build_and_run(&["--cp", &cp, "--entry", entry], &bin, &[arg])
}

/// What the executable built from test program `name`, main class `name`, prints.
fn run_main(name: &str) -> String {
    let bin = Path::new(env!("CARGO_TARGET_TMPDIR")).join("aot-main").join(name);
    let cp = program(name).to_string_lossy().into_owned();
    build_and_run(&["--cp", &cp, name], &bin, &[])
}

fn check(name: &str) -> String {
    std::fs::read_to_string(program(name).join(format!("{name}.check"))).expect("the .check file")
}

#[test]
fn hello_and_fib_run_through_their_main() {
    assert_eq!(run_main("hello"), check("hello"));
    assert_eq!(run_main("fib"), check("fib"));
}

#[test]
fn sources_referring_to_each_other_build_into_one_executable() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/aot/mutual");
    let bin = Path::new(env!("CARGO_TARGET_TMPDIR")).join("aot-src").join("mutual");
    let out = build_and_run(&["--main", "mutual", &dir.to_string_lossy()], &bin, &[]);
    assert_eq!(out, std::fs::read_to_string(dir.join("mutual.check")).expect("the .check file"));
}

#[test]
fn sources_use_a_dependency_from_the_coursier_cache() {
    if !in_coursier_cache(&["com.lihaoyi:geny_native0.5_3:1.1.0"]) {
        eprintln!("skipped: geny 1.1.0 is not in the Coursier cache");
        return;
    }
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/aot/deps");
    let bin = Path::new(env!("CARGO_TARGET_TMPDIR")).join("aot-src").join("deps");
    let out = build_and_run(&[&dir.to_string_lossy()], &bin, &[]);
    assert_eq!(out, std::fs::read_to_string(dir.join("deps.check")).expect("the .check file"));
}

#[test]
fn unit_compared_as_object_and_methods_of_null() {
    assert_eq!(run_main("nullrecv"), check("nullrecv"));
}

#[test]
fn link_time_methods_computed_from_others() {
    assert_eq!(run_main("linktime"), check("linktime"));
}

#[test]
fn fib_runs_as_executable() {
    assert_eq!(run("fib", "fibo", "25"), "75025\n");
}

#[test]
fn negative_literals_and_overflowing_division() {
    assert_eq!(run("intops", "calc", "20"), check("intops"));
}

#[test]
fn virtual_and_trait_dispatch_type_tests_and_modules() {
    assert_eq!(run("dispatch", "run", "20"), check("dispatch"));
}
