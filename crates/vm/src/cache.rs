//! Keeping the cache directory small: last-use marks, and removal of what went unused.

use std::fs::File;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// How long a library image or compiled script may go unused before it is removed.
pub const UNUSED: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// How long a half-written entry, left by a build that did not finish, is kept.
pub const ABANDONED: Duration = Duration::from_secs(24 * 60 * 60);

/// Marks `path` as used now; a directory by its `main` file.
pub fn touch(path: &Path) {
    let file = mark(path);
    let _ = File::options().write(true).open(file).and_then(|f| f.set_modified(SystemTime::now()));
}

/// Removes the entries of `dir` whose names `stale` accepts and that went unused for `age`.
pub fn prune(dir: &Path, age: Duration, stale: impl Fn(&str) -> bool) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for path in entries.flatten().map(|e| e.path()) {
        let named = path.file_name().and_then(|n| n.to_str()).is_some_and(&stale);
        let used =
            std::fs::metadata(mark(&path)).or_else(|_| std::fs::metadata(&path)).and_then(|m| m.modified());
        if !named || !used.is_ok_and(|t| t.elapsed().unwrap_or_default() >= age) {
            continue;
        }
        let _ = match path.is_dir() {
            true => std::fs::remove_dir_all(&path),
            false => std::fs::remove_file(&path),
        };
    }
}

/// The file whose modification time marks `path`'s last use.
fn mark(path: &Path) -> std::path::PathBuf {
    match path.is_dir() {
        true => path.join("main"),
        false => path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{ABANDONED, UNUSED, prune, touch};
    use std::fs::{self, File};
    use std::time::{Duration, SystemTime};

    fn age(path: &std::path::Path, days: u64) {
        let then = SystemTime::now() - Duration::from_secs(days * 24 * 60 * 60);
        File::options().write(true).open(path).and_then(|f| f.set_modified(then)).expect("a file");
    }

    #[test]
    fn prunes_what_went_unused() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tmp/cache-test")
            .join(std::process::id().to_string());
        fs::create_dir_all(dir.join("old-entry")).expect("a directory");
        fs::create_dir_all(dir.join("new-entry")).expect("a directory");
        for file in ["old.img", "used.img", "other.txt", "old-entry/main", "new-entry/main"] {
            fs::write(dir.join(file), b"x").expect("a file");
        }
        for file in ["old.img", "used.img", "other.txt", "old-entry/main"] {
            age(&dir.join(file), 40);
        }
        touch(&dir.join("used.img"));
        prune(&dir, UNUSED, |n| n.ends_with(".img") || n.ends_with("-entry"));
        prune(&dir, ABANDONED, |n| n == "missing");
        let left: Vec<bool> = ["old.img", "used.img", "other.txt", "old-entry", "new-entry"]
            .iter()
            .map(|f| dir.join(f).exists())
            .collect();
        assert_eq!(left, [false, true, true, false, true]);
        fs::remove_dir_all(&dir).expect("cleanup");
    }
}
