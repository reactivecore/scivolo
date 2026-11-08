//! The class files of the JDK that `java` runs (ADR 0106): its `lib/modules`, else its
//! `lib/ct.sym`.

use super::ClassInfo;
use super::jimage::Jimage;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

enum Source {
    Jimage(Jimage),
    /// `lib/ct.sym` and its entry of each class of the latest release it holds.
    CtSym {
        zip: ZipArchive<File>,
        entries: HashMap<String, String>,
    },
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
    Jimage::open(&home.join("lib/modules")).map(Source::Jimage).or_else(|| ct_sym(home))
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
            Source::Jimage(image) => image.bytes(name),
            Source::CtSym { zip, entries } => {
                let mut file = zip.by_name(entries.get(name)?).ok()?;
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes).ok()?;
                Some(bytes)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Jdk, Jimage, Source, ct_sym, home};

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
    fn reads_jdk_classes_from_jimage_and_ct_sym() {
        let Some(home) = home() else { return eprintln!("no JDK found: skipped") };
        let mut image = Jimage::open(&home.join("lib/modules")).expect("an uncompressed lib/modules");
        assert!(image.bytes("java/util/HexFormat").is_some(), "JDK 17's HexFormat");
        let image = Some(Source::Jimage(image));
        for source in [image, ct_sym(&home)].into_iter().flatten() {
            reads_jdk_classes(Jdk { source: Some(Some(source)), classes: Default::default() });
        }
    }
}
