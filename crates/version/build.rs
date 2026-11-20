//! Sets `SCIVOLO_VERSION`: the package version, followed by `+g` and the commit unless the
//! commit is tagged `v<version>` or the sources are not in a git checkout.

use std::path::PathBuf;
use std::process::Command;

/// Hex digits of the commit in the version.
const COMMIT_DIGITS: usize = 9;

fn main() {
    let version = std::env::var("CARGO_PKG_VERSION").expect("cargo sets CARGO_PKG_VERSION");
    let full = match git(&["rev-parse", &format!("--short={COMMIT_DIGITS}"), "HEAD"]) {
        None => version,
        Some(commit) => {
            watch_head();
            let tag = format!("v{version}");
            match git(&["tag", "--points-at", "HEAD", "--list", &tag]) {
                Some(t) if t == tag => version,
                _ => format!("{version}+g{commit}"),
            }
        }
    };
    println!("cargo::rustc-env=SCIVOLO_VERSION={full}");
}

/// Runs git with `args` in the crate's directory; its trimmed output if it succeeds.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Reruns this script when HEAD, the branch it names or a tag moves.
fn watch_head() {
    let mut paths = vec!["HEAD".to_string(), "packed-refs".into(), "refs/tags".into()];
    paths.extend(git(&["symbolic-ref", "-q", "HEAD"]));
    for path in paths {
        let Some(rel) = git(&["rev-parse", "--git-path", &path]) else { continue };
        let file = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default()).join(rel);
        if file.exists() {
            println!("cargo::rerun-if-changed={}", file.display());
        }
    }
}
