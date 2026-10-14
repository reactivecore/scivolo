//! Keeping the cache directory small: last-use marks, and removal of what went unused.

use std::fs::File;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// How long a compiled script may go unused before it is removed.
pub const UNUSED: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// How long a library image may go unused before building another removes it.
pub const IMAGE_UNUSED: Duration = Duration::from_secs(24 * 60 * 60);
/// How long a half-written entry, left by a build that did not finish, is kept.
pub const ABANDONED: Duration = Duration::from_secs(24 * 60 * 60);

/// Whether `s` is a cache key as the cache names its entries: 16 lowercase hex digits.
pub fn is_key(s: &str) -> bool {
    s.len() == 16 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Whether `s` is `key_prefix`, a cache key, `suffix` and a process id, as the cache names what
/// a build writes before moving it into place.
pub fn is_staging(s: &str, key_prefix: &str, suffix: &str) -> bool {
    s.strip_prefix(key_prefix).and_then(|rest| rest.split_at_checked(16)).is_some_and(|(key, rest)| {
        is_key(key)
            && rest
                .strip_prefix(suffix)
                .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// Marks file or directory `path` as used now.
pub fn touch(path: &Path) {
    let _ = File::open(path).and_then(|f| f.set_modified(SystemTime::now()));
}

/// Removes the entries of `dir` whose names `ours` accepts and that went unused for `age`,
/// without following symbolic links.
pub fn prune(dir: &Path, age: Duration, ours: impl Fn(&str) -> bool) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let named = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(&ours));
    for path in named {
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if !meta.modified().is_ok_and(|t| t.elapsed().unwrap_or_default() >= age) {
            continue;
        }
        let _ = if meta.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
    }
}

#[cfg(test)]
mod tests {
    use super::{ABANDONED, UNUSED, is_key, is_staging, prune, touch};
    use std::fs::{self, File};
    use std::time::{Duration, SystemTime};

    fn age(path: &std::path::Path, days: u64) {
        let then = SystemTime::now() - Duration::from_secs(days * 24 * 60 * 60);
        File::open(path).and_then(|f| f.set_modified(then)).expect("an entry");
    }

    #[test]
    fn names_are_ours_only() {
        assert!(is_key("0123456789abcdef"));
        assert!(!is_key("0123456789ABCDEF") && !is_key("deploy") && !is_key("0123456789abcde"));
        assert!(is_staging("0123456789abcdef.42", "", "."));
        assert!(is_staging("libs-0123456789abcdef.tmp7", "libs-", ".tmp"));
        assert!(!is_staging("0123456789abcdef.", "", ".") && !is_staging("backup.sh", "", "."));
    }

    #[test]
    fn prunes_what_went_unused() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tmp/cache-test")
            .join(std::process::id().to_string());
        let (old, used, young) = ("00000000000000aa", "00000000000000bb", "00000000000000cc");
        for entry in [old, used, young, "0000000000000dd0.1", "deploy"] {
            fs::create_dir_all(dir.join(entry)).expect("a directory");
        }
        fs::write(dir.join("notes"), b"x").expect("a file");
        for entry in [old, used, "0000000000000dd0.1", "deploy", "notes"] {
            age(&dir.join(entry), 40);
        }
        touch(&dir.join(used));
        prune(&dir, UNUSED, is_key);
        prune(&dir, ABANDONED, |n| is_staging(n, "", "."));
        let left: Vec<bool> = [old, used, young, "0000000000000dd0.1", "deploy", "notes"]
            .iter()
            .map(|e| dir.join(e).exists())
            .collect();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(left, [false, true, true, false, true, true]);
    }
}
