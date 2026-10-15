//! Links the object with the entry stub and Scala Native's C runtime through `cc`.

use std::path::Path;
use std::process::Command;

pub fn executable(object: &[u8], output: &Path) -> Result<(), String> {
    if let Some(dir) = output.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let obj = output.with_extension("o");
    std::fs::write(&obj, object).map_err(|e| format!("{}: {e}", obj.display()))?;
    let entry = Path::new(env!("CARGO_MANIFEST_DIR")).join("c/entry.c");
    let mut cmd = Command::new(std::env::var("CC").unwrap_or_else(|_| "cc".into()));
    cmd.arg("-o").arg(output).arg(&obj).arg(entry).arg(snrt::ARCHIVE).arg("-lz");
    if !cfg!(target_os = "macos") {
        cmd.args(["-lpthread", "-lm"]);
    }
    let status = cmd.status().map_err(|e| format!("cc: {e}"))?;
    if !status.success() {
        return Err(format!("cc failed: {status}"));
    }
    Ok(())
}
