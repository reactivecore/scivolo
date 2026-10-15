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

/// Links `object` and the C source `shims` with the C stub `stub` (`main.c` or `entry.c`) into
/// `output`.
pub fn executable(object: &[u8], shims: &str, stub: &str, output: &Path) -> Result<(), String> {
    if let Some(dir) = output.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let (obj, src) = (beside(output, ".scalai-aot.o"), beside(output, ".scalai-aot.c"));
    std::fs::write(&obj, object).map_err(|e| format!("{}: {e}", obj.display()))?;
    std::fs::write(&src, shims).map_err(|e| format!("{}: {e}", src.display()))?;
    let c = Path::new(env!("CARGO_MANIFEST_DIR")).join("c");
    let mut cmd = Command::new(std::env::var("CC").unwrap_or_else(|_| "cc".into()));
    cmd.args(["-w", "-o"])
        .arg(output)
        .arg(&obj)
        .arg(&src)
        .arg(c.join(stub))
        .arg(c.join("rt.c"))
        .arg(c.join("natives.c"));
    cmd.arg(snrt::ARCHIVE).arg("-lz");
    if !cfg!(target_os = "macos") {
        cmd.args(["-lpthread", "-lm"]);
    }
    let status = cmd.status().map_err(|e| format!("cc: {e}"));
    let _ = std::fs::remove_file(&obj);
    let _ = std::fs::remove_file(&src);
    if !status?.success() {
        return Err("cc failed".into());
    }
    Ok(())
}
