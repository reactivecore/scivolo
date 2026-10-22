//! The heap snapshot of the initialized runtime, in the cache directory beside the library
//! image it is built from (ADR 0034).

use libimage::{Image, cache};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use vm::Snapshot;

/// How the VM over library image `image` starts: from its snapshot file, or cold writing one
/// when it is missing. `SCIVOLO_SNAPSHOT=FILE` names another file; `SCIVOLO_NO_SNAPSHOT`
/// starts cold without writing.
pub fn choose(image: Option<&Image>) -> Snapshot {
    if std::env::var_os("SCIVOLO_NO_SNAPSHOT").is_some() {
        return Snapshot::Cold;
    }
    let (file, cached) = match (std::env::var_os("SCIVOLO_SNAPSHOT"), image) {
        (Some(file), _) => (PathBuf::from(file), false),
        (None, Some(image)) => (libs::cache_dir().join(name(image.key)), true),
        (None, None) => return Snapshot::Cold,
    };
    match std::fs::read(&file) {
        Ok(data) => {
            cache::touch(&file);
            Snapshot::Restore(data)
        }
        Err(_) => {
            if cached {
                prune(&libs::cache_dir());
            }
            Snapshot::Write(file)
        }
    }
}

/// The file name of the snapshot of image `key` for this build and platform.
fn name(key: u64) -> String {
    let mut h = DefaultHasher::new();
    (key, libs::BUILD, std::env::consts::OS, std::env::consts::ARCH).hash(&mut h);
    format!("snap-{:016x}.bin", h.finish())
}

/// Removes snapshots unused as long as library images are kept, and abandoned staging files.
fn prune(dir: &Path) {
    cache::prune(dir, cache::ABANDONED, |n| cache::is_staging(n, "snap-", ".tmp"));
    cache::prune(dir, cache::IMAGE_UNUSED, |n| {
        n.strip_prefix("snap-").and_then(|n| n.strip_suffix(".bin")).is_some_and(cache::is_key)
    });
}

#[cfg(test)]
mod tests {
    use super::name;

    #[test]
    fn names_are_snapshot_entries() {
        let n = name(7);
        assert!(n.starts_with("snap-") && n.ends_with(".bin") && n != name(8));
        assert!(libimage::cache::is_key(&n[5..21]));
    }
}
