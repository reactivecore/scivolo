//! The class files of the JDK that `java` runs (ADR 0106): its jmods, else its `lib/ct.sym`.

use super::ClassInfo;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

enum Source {
    /// The jmods not yet opened, `java.base` first, and those opened.
    Jmods { closed: Vec<PathBuf>, open: Vec<ZipArchive<File>> },
    /// `lib/ct.sym` and its entry of each class of the latest release it holds.
    CtSym { zip: ZipArchive<File>, entries: HashMap<String, String> },
}

/// The JDK's classes by internal name, the JDK found on the first lookup and each class read
/// once.
#[derive(Default)]
pub struct Jdk {
    source: Option<Option<Source>>,
    classes: HashMap<String, Option<ClassInfo>>,
}

impl Jdk {
    /// The JDK's class of internal name `name`, none if no JDK is found or it lacks the class.
    pub fn class(&mut self, name: &str) -> Option<&ClassInfo> {
        if !self.classes.contains_key(name) {
            let source = self.source.get_or_insert_with(|| open(&home()?));
            let info = source.as_mut().and_then(|s| s.bytes(name)).and_then(|b| super::read(&b).ok());
            self.classes.insert(name.to_owned(), info);
        }
        self.classes[name].as_ref()
    }
}

/// The JDK's directory: that of `JAVA_HOME`'s `java`, else of the `java` on the `PATH`, links
/// resolved.
fn home() -> Option<PathBuf> {
    let java_home = std::env::var_os("JAVA_HOME").map(|h| PathBuf::from(h).join("bin"));
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = java_home.into_iter().chain(std::env::split_paths(&path));
    let java = dirs.find_map(|d| Some(d.join("java")).filter(|j| j.is_file()))?;
    Some(java.canonicalize().ok()?.parent()?.parent()?.to_owned())
}

fn open(home: &Path) -> Option<Source> {
    jmods(home).or_else(|| ct_sym(home))
}

fn jmods(home: &Path) -> Option<Source> {
    let jmods = std::fs::read_dir(home.join("jmods")).into_iter().flatten().flatten();
    let mut closed: Vec<PathBuf> =
        jmods.map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "jmod")).collect();
    closed.sort_by_key(|p| std::cmp::Reverse((!p.ends_with("java.base.jmod"), p.clone())));
    let base = closed.last().is_some_and(|p| p.ends_with("java.base.jmod"));
    base.then_some(Source::Jmods { closed, open: Vec::new() })
}

fn ct_sym(home: &Path) -> Option<Source> {
    let zip = ZipArchive::new(File::open(home.join("lib/ct.sym")).ok()?).ok()?;
    let names = zip.file_names().flatten().filter(|n| n.ends_with(".sig")).map(|n| n.into_owned());
    let names: Vec<String> = names.collect();
    let release = |n: &str| n.split_once('/').map_or("", |(dir, _)| dir).to_owned();
    let latest = names.iter().flat_map(|n| release(n).chars().max()).max()?;
    let entries = names
        .into_iter()
        .filter(|n| release(n).contains(latest))
        .filter_map(|n| {
            let (_, in_module) = n.split_once('/')?.1.split_once('/')?;
            Some((in_module.strip_suffix(".sig")?.to_owned(), n.clone()))
        })
        .collect();
    Some(Source::CtSym { zip, entries })
}

impl Source {
    /// The bytes of the class file of class `name`.
    fn bytes(&mut self, name: &str) -> Option<Vec<u8>> {
        match self {
            Source::CtSym { zip, entries } => entry(zip, entries.get(name)?),
            Source::Jmods { closed, open } => {
                let entry_name = format!("classes/{name}.class");
                if let Some(bytes) = open.iter_mut().find_map(|z| entry(z, &entry_name)) {
                    return Some(bytes);
                }
                while let Some(path) = closed.pop() {
                    let Some(mut z) = File::open(path).ok().and_then(|f| ZipArchive::new(f).ok()) else {
                        continue;
                    };
                    let bytes = entry(&mut z, &entry_name);
                    open.push(z);
                    if bytes.is_some() {
                        return bytes;
                    }
                }
                None
            }
        }
    }
}

fn entry(zip: &mut ZipArchive<File>, name: &str) -> Option<Vec<u8>> {
    let mut file = zip.by_name(name).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::{Jdk, ct_sym, home, jmods};

    fn reads_jdk_classes(mut jdk: Jdk) {
        let list = jdk.class("java/util/ArrayList").expect("ArrayList");
        assert_eq!(list.superclass.as_deref(), Some("java/util/AbstractList"));
        assert!(list.interfaces.iter().any(|i| i == "java/util/List"));
        assert!(list.methods.iter().any(|m| m.name == "add" && m.descriptor == "(Ljava/lang/Object;)Z"));
        let date = jdk.class("java/sql/Date").expect("java.sql.Date");
        assert_eq!(date.superclass.as_deref(), Some("java/util/Date"));
        assert!(jdk.class("scala/Predef").is_none());
    }

    #[test]
    fn reads_jdk_classes_from_jmods_and_ct_sym() {
        let Some(home) = home() else { return eprintln!("no JDK found: skipped") };
        let sources: Vec<_> = [jmods(&home), ct_sym(&home)].into_iter().flatten().collect();
        assert!(!sources.is_empty(), "neither jmods nor lib/ct.sym in {home:?}");
        for source in sources {
            reads_jdk_classes(Jdk { source: Some(Some(source)), classes: Default::default() });
        }
    }
}
