//! The inputs of a build: Scala sources from files and directories, and the main class.

use crate::R;
use std::path::{Path, PathBuf};

/// Whether input `arg` names sources rather than a main class: a `.scala` file or a directory.
pub fn is_source(arg: &str) -> bool {
    arg.ends_with(".scala") || Path::new(arg).is_dir()
}

/// The `.scala` files `inputs` name and those under the directories among them, sorted.
pub fn collect(inputs: &[String]) -> R<Vec<PathBuf>> {
    let mut files = Vec::new();
    for input in inputs {
        let path = Path::new(input);
        if path.is_dir() {
            under(path, &mut files)?;
        } else if path.is_file() {
            files.push(path.to_owned());
        } else {
            return Err(format!("{input}: no such file or directory"));
        }
    }
    files.sort();
    files.dedup();
    match files.is_empty() {
        true => Err(format!("no .scala files in {}", inputs.join(", "))),
        false => Ok(files),
    }
}

fn under(dir: &Path, files: &mut Vec<PathBuf>) -> R<()> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            under(&path, files)?;
        } else if path.extension().is_some_and(|e| e == "scala") {
            files.push(path);
        }
    }
    Ok(())
}

/// The main class among `mains`: `chosen`, else the only one.
pub fn pick(mains: &[String], chosen: Option<&str>) -> R<String> {
    let list = mains.join(", ");
    match chosen {
        Some(c) if mains.iter().any(|m| m == c) => Ok(c.to_owned()),
        Some(c) => Err(format!("no main class {c}; the sources have {list}")),
        None if mains.len() == 1 => Ok(mains[0].clone()),
        None if mains.is_empty() => Err("no main method in the sources".into()),
        None => Err(format!("several main classes, choose one with --main: {list}")),
    }
}

#[cfg(test)]
mod tests {
    use super::pick;

    #[test]
    fn picks_the_only_or_the_chosen_main() {
        let mains = ["one".to_owned(), "two".to_owned()];
        assert_eq!(pick(&mains[..1], None), Ok("one".to_owned()));
        assert_eq!(pick(&mains, Some("two")), Ok("two".to_owned()));
        assert!(pick(&mains, None).unwrap_err().contains("--main"));
        assert!(pick(&mains, Some("three")).is_err() && pick(&[], None).is_err());
    }
}
