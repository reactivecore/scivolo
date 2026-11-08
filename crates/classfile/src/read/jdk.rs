//! The class files of the JDK that `java` runs (ADR 0106): its `lib/modules`, else its
//! `lib/ct.sym`.

use super::ClassInfo;
use super::jimage::Jimage;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
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
    packages: Option<HashSet<String>>,
}

impl Jdk {
    /// The JDK's class of internal name `name`, none if no JDK is found or it lacks the class.
    pub fn class(&mut self, name: &str) -> Option<&ClassInfo> {
        if !self.classes.contains_key(name) {
            let info = self.source().and_then(|s| s.bytes(name)).and_then(|b| super::read(&b).ok());
            self.classes.insert(name.to_owned(), info);
        }
        self.classes[name].as_ref()
    }

    /// Whether the JDK has a class in package `pkg`, an internal name such as `java/time`.
    pub fn has_package(&mut self, pkg: &str) -> bool {
        if self.packages.is_none() {
            let names = self.source().map_or_else(Vec::new, |s| s.names());
            let mut packages = HashSet::new();
            for name in names {
                let mut pkg = name.as_str();
                while let Some((outer, _)) = pkg.rsplit_once('/') {
                    pkg = outer;
                    if !packages.insert(pkg.to_owned()) {
                        break;
                    }
                }
            }
            self.packages = Some(packages);
        }
        self.packages.as_ref().is_some_and(|p| p.contains(pkg))
    }

    fn source(&mut self) -> Option<&mut Source> {
        self.source.get_or_insert_with(|| open(&home()?)).as_mut()
    }
}

/// The JDK's directory: the `java.home` of `JAVA_HOME`'s `java`, else of the `java` on the
/// `PATH` (a launcher stub such as macOS's `/usr/bin/java` reports the JDK it starts), else the
/// directory above the `java` binary, links resolved.
fn home() -> Option<PathBuf> {
    let java_home = std::env::var_os("JAVA_HOME").map(|h| PathBuf::from(h).join("bin"));
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = java_home.into_iter().chain(std::env::split_paths(&path));
    let java = dirs.find_map(|d| Some(d.join("java")).filter(|j| j.is_file()))?;
    let beside = || Some(java.canonicalize().ok()?.parent()?.parent()?.to_owned());
    let has_classes = |h: &PathBuf| ["lib/modules", "lib/ct.sym"].iter().any(|f| h.join(f).is_file());
    reported_home(&java).or_else(beside).filter(has_classes)
}

/// The `java.home` property `java` reports.
fn reported_home(java: &Path) -> Option<PathBuf> {
    let out = Command::new(java).args(["-XshowSettings:properties", "-version"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stderr);
    let line = text.lines().find_map(|l| l.trim().strip_prefix("java.home = "))?;
    Some(PathBuf::from(line))
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
    /// The internal names of the classes.
    fn names(&self) -> Vec<String> {
        match self {
            Source::Jimage(image) => image.names(),
            Source::CtSym { entries, .. } => entries.keys().cloned().collect(),
        }
    }

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
        let date = jdk.class("java/time/LocalDate").expect("LocalDate");
        let dates = date.methods.iter().find(|m| m.name == "datesUntil").expect("datesUntil");
        assert_eq!(
            dates.signature.as_deref(),
            Some("(Ljava/time/LocalDate;)Ljava/util/stream/Stream<Ljava/time/LocalDate;>;")
        );
        let list = jdk.class("java/util/ArrayList").expect("ArrayList");
        assert!(list.signature.as_deref().is_some_and(|s| s.starts_with("<E:Ljava/lang/Object;>")));
        assert!(
            jdk.has_package("java/time")
                && jdk.has_package("java")
                && !jdk.has_package("java/time/LocalDate")
        );
    }

    #[test]
    fn reads_jdk_classes_from_jimage_and_ct_sym() {
        let Some(home) = home() else { return eprintln!("no JDK found: skipped") };
        let modules = home.join("lib/modules");
        let mut image =
            Jimage::open(&modules).unwrap_or_else(|| panic!("an uncompressed {}", modules.display()));
        assert!(image.bytes("java/util/HexFormat").is_some(), "JDK 17's HexFormat");
        let image = Some(Source::Jimage(image));
        for source in [image, ct_sym(&home)].into_iter().flatten() {
            reads_jdk_classes(Jdk { source: Some(Some(source)), ..Default::default() });
        }
    }
}
