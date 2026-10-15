//! Links the object with the entry stub, the lookup helpers and Scala Native's C runtime
//! through `cc`.

use std::path::Path;
use std::process::Command;

/// Links `object` with the C stub `stub` (`main.c` or `entry.c`) into `output`.
pub fn executable(object: &[u8], stub: &str, output: &Path) -> Result<(), String> {
    if let Some(dir) = output.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let mut obj = output.as_os_str().to_owned();
    obj.push(".scalai-aot.o");
    let obj = std::path::PathBuf::from(obj);
    std::fs::write(&obj, object).map_err(|e| format!("{}: {e}", obj.display()))?;
    let c = Path::new(env!("CARGO_MANIFEST_DIR")).join("c");
    let mut cmd = Command::new(std::env::var("CC").unwrap_or_else(|_| "cc".into()));
    cmd.arg("-o").arg(output).arg(&obj).arg(c.join(stub)).arg(c.join("rt.c"));
    cmd.arg(snrt::ARCHIVE).arg("-lz");
    if !cfg!(target_os = "macos") {
        cmd.args(["-lpthread", "-lm"]);
    }
    let status = cmd.status().map_err(|e| format!("cc: {e}"));
    let _ = std::fs::remove_file(&obj);
    if !status?.success() {
        return Err("cc failed".into());
    }
    Ok(())
}
