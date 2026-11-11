//! The Java classes of the jars' class files, entered on lookup with their nested classes
//! (ADR 0117).

use super::super::{SymbolId, Universe, module_class_name};
use super::class;
use crate::types::NameId;
use classfile::{ACC_MODULE, ACC_PROTECTED, ACC_PUBLIC, ACC_STATIC, ACC_SYNTHETIC, ClassInfo};
use rustc_hash::{FxHashMap, FxHashSet};

/// The Java class files of the jars: the jar of each class by internal name, and each class read
/// once.
#[derive(Default)]
pub struct JavaClasses {
    jars: FxHashMap<String, usize>,
    read: FxHashMap<String, Option<ClassInfo>>,
}

impl Universe {
    /// Makes the Java classes of the jars' class files known and enters their packages: those of
    /// no TASTy file, outside `java.*` and `javax.*`.
    pub fn use_java_classfiles(&mut self) -> std::io::Result<()> {
        let mut jars = self.library.java_classes()?;
        jars.retain(|name, _| !name.starts_with("java/") && !name.starts_with("javax/"));
        let packages: FxHashSet<String> =
            jars.keys().filter_map(|n| n.rsplit_once('/')).map(|(p, _)| p.replace('/', ".")).collect();
        for p in packages {
            self.enter_package(&p);
        }
        self.java_classes = Some(JavaClasses { jars, read: FxHashMap::default() });
        Ok(())
    }

    /// Enters public class `name` of package `pkg` (an object `C$` stands for its class `C`) from
    /// a jar's class file with its nested classes, if the universe uses the jars' class files and
    /// the package lacks the class and its object: the declarations named `name` entered.
    pub fn classpath_decl(&mut self, pkg: SymbolId, name: NameId) -> Vec<SymbolId> {
        if self.java_classes.is_none() {
            return Vec::new();
        }
        let name = self.name(name).to_owned();
        let class = name.strip_suffix('$').unwrap_or(&name);
        let outer = self.full_name(pkg).replace('.', "/");
        let internal = if outer.is_empty() { class.to_owned() } else { format!("{outer}/{class}") };
        let names = [class.to_owned(), module_class_name(class)];
        let defined = self.symbol(pkg).decls.iter().any(|&d| names.contains(&self.symbol(d).name));
        if class.contains(['$', '/']) || defined {
            return Vec::new();
        }
        let Some(info) = java_class(self, &internal) else { return Vec::new() };
        if info.access & ACC_PUBLIC == 0 || info.access & ACC_MODULE != 0 {
            return Vec::new();
        }
        let entered = enter(self, pkg, class, &info);
        entered.into_iter().filter(|&d| self.symbol(d).name == name).collect()
    }
}

/// Enters class `info` into `owner` as `name` with its public and protected nested classes, a
/// static one into the object's class (dotty's `enterOwnInnerClasses`): the class, its object's
/// class and its object, none if its signature is refused.
fn enter(u: &mut Universe, owner: SymbolId, name: &str, info: &ClassInfo) -> Vec<SymbolId> {
    let Some(entered) = class::enter(u, owner, name, info) else { return Vec::new() };
    for nested in &info.inner_classes {
        let (Some(outer), Some(simple)) = (&nested.outer, &nested.name) else { continue };
        let visible = nested.flags & (ACC_PUBLIC | ACC_PROTECTED) != 0 && nested.flags & ACC_SYNTHETIC == 0;
        if *outer != info.name || !visible {
            continue;
        }
        let Some(inner) = java_class(u, &nested.inner) else { continue };
        let owner = if nested.flags & ACC_STATIC != 0 { entered[1] } else { entered[0] };
        enter(u, owner, simple, &inner);
    }
    entered
}

/// Reads the class of internal name `internal` from the jars' class files; none if they lack it,
/// it is no class file or one compiled from Scala.
pub fn java_class(u: &mut Universe, internal: &str) -> Option<ClassInfo> {
    let classes = u.java_classes.as_mut()?;
    if let Some(info) = classes.read.get(internal) {
        return info.clone();
    }
    let jar = *classes.jars.get(internal)?;
    let info = u.library.read_class(jar, internal).ok().and_then(|b| classfile::read(&b).ok());
    let info = info.filter(|i| !i.scala);
    u.java_classes.as_mut()?.read.insert(internal.to_owned(), info.clone());
    info
}
