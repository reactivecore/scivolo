//! A whole file compiled and run as the browser's file mode runs it.
#![cfg(not(feature = "portable"))]

use repl::{Ran, compile_program, run_program};
use vm::{Classpath, Vm};

const FILE: &str = "main.scala";

fn failed(ran: Result<repl::Program, Ran>) -> Ran {
    match ran {
        Ok(p) => panic!("{} compiled", p.main),
        Err(ran) => ran,
    }
}

fn run() -> Result<(), String> {
    let jars = libs::jars()?;
    let image = libs::image(&jars).map_err(|e| e.to_string())?;
    let classpath = Classpath::with_image(&[], image).map_err(|e| e.to_string())?;
    let vm = &mut Vm::new(classpath).map_err(|e| e.to_string())?;

    let mismatch = "@main def m(): Unit =\n  val x: Int = \"a\"\n  println(x)\n";
    let ran = failed(compile_program((FILE, mismatch), image, 80, vm));
    assert_eq!((ran.exit, ran.error_lines.as_slice()), (1, &[2][..]), "{:?}", ran.lines);
    assert!(ran.lines.iter().any(|l| l.contains("Type Mismatch Error")), "{:?}", ran.lines);
    assert_eq!(ran.lines.last().map(String::as_str), Some("1 error found"));

    let ran = failed(compile_program((FILE, "object A\n"), image, 80, vm));
    assert_eq!(ran.lines, ["main.scala: no main method"]);

    let ran = failed(compile_program((FILE, "//> using dep a::b:1\nobject A\n"), image, 80, vm));
    assert!(ran.lines[0].starts_with("-- unsupported: dependency a::b:1"), "{:?}", ran.lines);

    let script = "def helper(x: Int): Int = {\n  x + 1\n}\n\nprintln(\"Foo \" + helper(3))\n";
    let program = compile_program((FILE, script), image, 80, vm).map_err(|r| r.lines.join("\n"))?;
    assert_eq!((program.main.as_str(), program.warnings.as_slice()), ("main_sc", &[][..]));

    let script_error = "val a = 1\nprintln(a)\nval b: Int = \"x\"\n";
    let ran = failed(compile_program((FILE, script_error), image, 80, vm));
    assert_eq!(ran.error_lines, [3], "{:?}", ran.lines);
    assert!(ran.lines.iter().any(|l| l.contains("3 |val b: Int = \"x\"")), "{:?}", ran.lines);

    let throws = "@main def hello(): Unit =\n  throw RuntimeException(\"boom\")\n";
    let program = compile_program((FILE, throws), image, 80, vm).map_err(|r| r.lines.join("\n"))?;
    assert_eq!((program.main.as_str(), vm.macro_expansions()), ("hello", 0));
    let ran = run_program(vm, program);
    assert_eq!(ran.exit, 1, "{:?}", ran.lines);
    let first = "scivolo: uncaught exception: java.lang.RuntimeException: boom";
    assert_eq!(ran.lines.first().map(String::as_str), Some(first), "{:?}", ran.lines);
    Ok(())
}

#[test]
fn a_file_compiles_and_runs_or_reports_its_errors() {
    let run = std::thread::Builder::new().stack_size(1 << 30).spawn(run).unwrap().join().unwrap();
    match run {
        Err(e) if e.contains("or the Coursier cache") && std::env::var_os("CI").is_none() => {
            eprintln!("skipped: Scala Native libraries are not in the Coursier cache");
        }
        run => run.unwrap(),
    }
}
