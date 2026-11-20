//! Sets `SCIVOLO_VERSION`: the package version, followed by `+g` and the commit unless the
//! commit is tagged `v<version>` or the workspace is not a git checkout of its own.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Hex digits of the commit in the version.
const COMMIT_DIGITS: usize = 9;

/// The commit the checkout is at.
const HEAD: &str = "HEAD";

fn main() {
    let version = std::env::var("CARGO_PKG_VERSION").expect("cargo sets CARGO_PKG_VERSION");
    let full = match commit() {
        None => version,
        Some(commit) => {
            watch_head();
            let tag = format!("v{version}");
            match git(&["tag", "--points-at", HEAD, "--list", &tag]) {
                Some(t) if t == tag => version,
                _ => format!("{version}+g{commit}"),
            }
        }
    };
    println!("cargo::rustc-env=SCIVOLO_VERSION={full}");
}

/// Determines the commit of the workspace's checkout: none if the workspace is not the top level
/// of a git checkout (e.g. a source archive), with a warning if it is one but git fails.
fn commit() -> Option<String> {
    let root = workspace_root();
    let top = git(&["rev-parse", "--show-toplevel"]).map(PathBuf::from);
    let commit = git(&["rev-parse", &format!("--short={COMMIT_DIGITS}"), HEAD]);
    let own = top.is_some_and(|t| t.canonicalize().ok() == root.canonicalize().ok());
    match (own, commit) {
        (true, Some(commit)) => Some(commit),
        _ if root.join(".git").exists() => {
            println!("cargo::warning=git failed in {}; the version names no commit", root.display());
            None
        }
        _ => None,
    }
}

/// Runs git with `args` in the crate's directory; its trimmed output if it succeeds.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Reruns this script when HEAD, the branch it names or a tag moves.
fn watch_head() {
    let mut paths = [HEAD, "logs/HEAD", "packed-refs", "refs/tags"].map(String::from).to_vec();
    paths.extend(git(&["symbolic-ref", "-q", HEAD]));
    for path in paths {
        let Some(rel) = git(&["rev-parse", "--git-path", &path]) else { continue };
        let file = manifest_dir().join(rel);
        if file.exists() {
            println!("cargo::rerun-if-changed={}", file.display());
        }
    }
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"))
}

/// The workspace's directory: `crates/version/../..`.
fn workspace_root() -> PathBuf {
    manifest_dir().ancestors().nth(2).map_or_else(|| manifest_dir(), Path::to_path_buf)
}
