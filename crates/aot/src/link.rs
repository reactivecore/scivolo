//! Links the object with its variadic call shims, the C stub, the runtime helpers and Scala
//! Native's C runtime through `cc`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// `output` with `suffix` appended to its file name.
fn beside(output: &Path, suffix: &str) -> PathBuf {
    let mut path = output.as_os_str().to_owned();
    path.push(suffix);
    PathBuf::from(path)
}

/// The C compiler, keeping frame pointers for the stack walk of `c/trace.c`.
fn cc() -> Command {
    let mut cmd = Command::new(std::env::var("CC").unwrap_or_else(|_| "cc".into()));
    cmd.arg("-fno-omit-frame-pointer");
    cmd
}

fn run(cmd: &mut Command, what: &str) -> Result<(), String> {
    let status = cmd.status().map_err(|e| format!("{what}: {e}"))?;
    match status.success() {
        true => Ok(()),
        false => Err(format!("{what} failed")),
    }
}

/// Links `object` and the C source `shims` with the C stub `stub` (`main.c` or `entry.c`) into
/// `output`.
pub fn executable(object: &[u8], shims: &str, stub: &str, output: &Path) -> Result<(), String> {
    if let Some(dir) = output.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let (obj, src, shim_obj) =
        (beside(output, ".scalai-aot.o"), beside(output, ".scalai-aot.c"), beside(output, ".shims.o"));
    std::fs::write(&obj, object).map_err(|e| format!("{}: {e}", obj.display()))?;
    std::fs::write(&src, shims).map_err(|e| format!("{}: {e}", src.display()))?;
    let c = Path::new(env!("CARGO_MANIFEST_DIR")).join("c");
    let shimmed = run(cc().args(["-w", "-c", "-o"]).arg(&shim_obj).arg(&src), "cc (shims)");
    let mut cmd = cc();
    cmd.arg("-o").arg(output).arg(&obj).arg(&shim_obj);
    for file in [stub, "rt.c", "natives.c", "stack.c", "trace.c", "resources.c"] {
        cmd.arg(c.join(file));
    }
    cmd.arg(snrt::ARCHIVE).arg("-lz");
    if !cfg!(target_os = "macos") {
        cmd.args(["-lpthread", "-lm"]);
    }
    let linked = shimmed.and_then(|()| run(&mut cmd, "cc"));
    for file in [&obj, &src, &shim_obj] {
        let _ = std::fs::remove_file(file);
    }
    linked
}
