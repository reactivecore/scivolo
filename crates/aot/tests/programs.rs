use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Every compile and run stops after this long.
const TIMEOUT: Duration = Duration::from_secs(20);

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

/// The output of `cmd`, which fails the test with `what` when it runs longer than `TIMEOUT`.
fn output(cmd: &mut Command, what: &str) -> Output {
    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect(what);
    let stdout = drain(child.stdout.take().expect("piped stdout"));
    let stderr = drain(child.stderr.take().expect("piped stderr"));
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect(what) {
            break status;
        }
        if start.elapsed() > TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            panic!("timeout ({what})");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    Output { status, stdout: stdout.join().expect(what), stderr: stderr.join().expect(what) }
}

/// What the executable `scalai-aot` builds with `args` into `bin` prints when run with `run`.
fn build_and_run(args: &[&str], bin: &Path, run: &[&str]) -> String {
    let mut compile = Command::new(env!("CARGO_BIN_EXE_scalai-aot"));
    compile.args(args).arg("-o").arg(bin);
    let out = output(&mut compile, "compile");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let out = output(Command::new(bin).args(run), "run");
    String::from_utf8_lossy(&out.stdout).into_owned()
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
