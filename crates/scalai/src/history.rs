//! The REPL's input history in JLine's file format (`<epoch millis>:<escaped line>`).

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The entries kept in memory.
pub const MEMORY: usize = 500;
/// The entries a trimmed file keeps.
const FILE_MAX: usize = 10_000;
/// The number of lines above which the file is trimmed.
const TRIM_ABOVE: usize = FILE_MAX + FILE_MAX / 4;

/// The history file Scala's own REPL uses, `~/.dotty_history`.
pub fn path() -> Option<PathBuf> {
    std::env::home_dir().filter(|h| !h.as_os_str().is_empty()).map(|h| h.join(".dotty_history"))
}

/// The entries of the file at `path`, oldest first; rewrites the file without the lines that
/// are not entries.
pub fn load(path: &Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
    let lines = lines(&bytes);
    let entries: Vec<&[u8]> = lines.iter().copied().filter(|l| entry(l).is_some()).collect();
    if entries.len() < lines.len() {
        let _ = rewrite(path, &entries);
    }
    entries.into_iter().filter_map(entry).collect()
}

/// The entry that input `line` adds after entry `last`, if any.
pub fn accept(last: Option<&str>, line: &str) -> Option<String> {
    if line.starts_with(' ') {
        return None;
    }
    let line = java_trim(line);
    (!line.is_empty() && last != Some(line)).then(|| line.to_string())
}

/// Appends `line` to the file at `path`, then trims the file if needed.
pub fn append(path: &Path, line: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let mut file = OpenOptions::new().read(true).append(true).create(true).open(path)?;
    let mut record = String::new();
    if file.metadata()?.len() > 0 {
        let mut last = [0];
        file.seek(SeekFrom::End(-1))?;
        file.read_exact(&mut last)?;
        if !matches!(last[0], b'\n' | b'\r') {
            record.push('\n');
        }
    }
    record.push_str(&format!("{millis}:{}\n", escape(line)));
    file.write_all(record.as_bytes())?;
    trim_if_needed(path)
}

/// Trims the file at `path` to its last `FILE_MAX` distinct entries when it holds more than
/// `TRIM_ABOVE` lines; of equal entries (trimmed), the last stays.
fn trim_if_needed(path: &Path) -> io::Result<()> {
    let bytes = std::fs::read(path)?;
    let lines = lines(&bytes);
    if lines.len() <= TRIM_ABOVE {
        return Ok(());
    }
    let mut seen = HashSet::new();
    let mut kept: Vec<&[u8]> = lines
        .iter()
        .rev()
        .filter(|l| entry(l).is_some_and(|e| seen.insert(java_trim(&e).to_string())))
        .take(FILE_MAX)
        .copied()
        .collect();
    kept.reverse();
    rewrite(path, &kept)
}

/// Replaces the file at `path` (behind symbolic links) by `lines`, through a new temporary
/// file with the original's permissions; fails when the file is not writable.
fn rewrite(path: &Path, lines: &[&[u8]]) -> io::Result<()> {
    let target = std::fs::canonicalize(path)?;
    OpenOptions::new().write(true).open(&target)?;
    let mut temp = target.clone().into_os_string();
    temp.push(format!(".{}.tmp", std::process::id()));
    let temp = PathBuf::from(temp);
    let file = OpenOptions::new().write(true).create_new(true).open(&temp)?;
    let written = replace(file, &temp, &target, lines);
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

/// Writes `lines` to `file` at `temp`, then renames it to `target`.
fn replace(mut file: std::fs::File, temp: &Path, target: &Path, lines: &[&[u8]]) -> io::Result<()> {
    let mut contents = Vec::new();
    for line in lines {
        contents.extend_from_slice(line);
        contents.push(b'\n');
    }
    file.set_permissions(std::fs::metadata(target)?.permissions())?;
    file.write_all(&contents)?;
    file.sync_all()?;
    std::fs::rename(temp, target)
}

/// The lines of `bytes`, each ended by LF, CR or CR LF, as Java's `lines()` splits.
fn lines(bytes: &[u8]) -> Vec<&[u8]> {
    let (mut out, mut start, mut i) = (Vec::new(), 0, 0);
    while i < bytes.len() {
        if matches!(bytes[i], b'\n' | b'\r') {
            out.push(&bytes[start..i]);
            i += if bytes[i..].starts_with(b"\r\n") { 2 } else { 1 };
            start = i;
        } else {
            i += 1;
        }
    }
    if start < bytes.len() {
        out.push(&bytes[start..]);
    }
    out
}

/// The entry of file line `line`: an integer, `:`, the escaped text, which does not end in an
/// odd number of `\`.
fn entry(line: &[u8]) -> Option<String> {
    let colon = line.iter().position(|&b| b == b':')?;
    std::str::from_utf8(&line[..colon]).ok()?.parse::<i64>().ok()?;
    let text = &line[colon + 1..];
    if text.iter().rev().take_while(|&&b| b == b'\\').count() % 2 == 1 {
        return None;
    }
    Some(unescape(&String::from_utf8_lossy(text)))
}

/// `s` without the leading and trailing characters up to U+0020, as Java's `String.trim`.
fn java_trim(s: &str) -> &str {
    s.trim_matches(|c| c <= ' ')
}

fn escape(s: &str) -> String {
    s.chars().fold(String::with_capacity(s.len()), |mut out, c| {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
        out
    })
}

fn unescape(s: &str) -> String {
    let (mut out, mut chars) = (String::with_capacity(s.len()), s.chars());
    while let Some(c) = chars.next() {
        out.push(match c {
            '\\' => match chars.next() {
                Some('n') => '\n',
                Some('r') => '\r',
                Some(c) => c,
                None => '\\',
            },
            c => c,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh file `name` under the repository's tmp/, holding `contents`.
    fn file(name: &str, contents: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp/history-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn rejects_an_odd_trailing_backslash() {
        let path = file("backslash", "1:a\\\n2:b\\\\\n3:c\\\\\\\n");
        assert_eq!(load(&path), ["b\\"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "2:b\\\\\n");
    }

    #[test]
    fn a_lone_carriage_return_ends_a_line() {
        let path = file("cr", "1:a\r2:b\r\n3:c\r");
        assert_eq!(load(&path), ["a", "b", "c"]);
        append(&path, "d").unwrap();
        assert_eq!(load(&path), ["a", "b", "c", "d"]);
    }

    #[test]
    fn leaves_a_read_only_file_alone() {
        use std::os::unix::fs::PermissionsExt;
        let path = file("read-only", "1:a\nbad\n");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o400)).unwrap();
        assert_eq!(load(&path), ["a"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "1:a\nbad\n");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    #[test]
    fn does_not_follow_an_existing_temporary_file() {
        let path = file("planted", "1:a\nbad\n");
        let other = file("planted-target", "x");
        let mut temp = std::fs::canonicalize(&path).unwrap().into_os_string();
        temp.push(format!(".{}.tmp", std::process::id()));
        let _ = std::fs::remove_file(&temp);
        std::os::unix::fs::symlink(&other, &temp).unwrap();
        assert_eq!(load(&path), ["a"]);
        assert_eq!(std::fs::read_to_string(&other).unwrap(), "x");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "1:a\nbad\n");
        assert!(std::fs::symlink_metadata(&temp).unwrap().file_type().is_symlink());
        std::fs::remove_file(&temp).unwrap();
    }

    #[test]
    fn escaping_round_trips() {
        let text = "a\\b\nc\rd\\n";
        assert_eq!(escape(text), "a\\\\b\\nc\\rd\\\\n");
        assert_eq!(unescape(&escape(text)), text);
        assert_eq!(unescape("x\\"), "x\\");
    }

    #[test]
    fn loads_entries() {
        let path = file("load", "1700000000000:val x = 1\n1700000000001:def f =\\n  2\n");
        assert_eq!(load(&path), ["val x = 1", "def f =\n  2"]);
    }

    #[test]
    fn rewrites_without_bad_lines() {
        let path = file("bad", "1:a\nno colon\nx1:b\n\n2:c");
        assert_eq!(load(&path), ["a", "c"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "1:a\n2:c\n");
    }

    #[test]
    fn rewrites_behind_a_link() {
        let target = file("target", "1:a\nbad\n");
        let link = target.with_file_name("link");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(load(&link), ["a"]);
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "1:a\n");
    }

    #[test]
    fn keeps_a_good_file_untouched() {
        let contents = "1:a\r\n2:\\\\b\n3:c";
        let path = file("good", contents);
        assert_eq!(load(&path), ["a", "\\b", "c"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), contents);
    }

    #[test]
    fn accepts_as_jline() {
        assert_eq!(accept(None, " x"), None);
        assert_eq!(accept(None, "x  \t"), Some("x".into()));
        assert_eq!(accept(Some("x"), "x "), None);
        assert_eq!(accept(Some("x"), "y"), Some("y".into()));
        assert_eq!(accept(None, "\t"), None);
    }

    #[test]
    fn appends_what_load_reads() {
        let path = file("append", "1:a");
        append(&path, "b\nc\\").unwrap();
        assert_eq!(load(&path), ["a", "b\nc\\"]);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("1:a\n") && text.ends_with(":b\\nc\\\\\n"), "{text}");
    }

    #[test]
    fn creates_the_file() {
        let path = file("create", "");
        std::fs::remove_file(&path).unwrap();
        append(&path, "x").unwrap();
        assert_eq!(load(&path), ["x"]);
    }

    #[test]
    fn trims_to_the_last_distinct_entries() {
        let mut contents = String::new();
        for i in 1..12_500 {
            let text = if i == 12_000 { "dup".to_string() } else { format!("e{i}") };
            contents.push_str(&format!("{i}:{text}\n"));
        }
        let path = file("trim", &contents);
        append(&path, "x").unwrap();
        assert_eq!(load(&path).len(), 12_500);
        append(&path, " dup ").unwrap();
        let entries = load(&path);
        assert_eq!(entries.len(), FILE_MAX);
        assert_eq!(entries[0], "e2501");
        assert_eq!(entries[FILE_MAX - 3..], ["e12499", "x", " dup "]);
        assert_eq!(entries.iter().filter(|e| e.trim() == "dup").count(), 1);
    }
}
