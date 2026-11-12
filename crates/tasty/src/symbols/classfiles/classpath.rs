//! The Java classes of the jars' class files, entered on lookup with their nested classes
//! (ADR 0117).

use super::super::{SymbolId, Universe, module_class_name};
use super::class;
use crate::types::NameId;
use classfile::{ACC_MODULE, ACC_PRIVATE, ACC_PROTECTED, ACC_PUBLIC, ACC_STATIC, ClassInfo};
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

/// The Java class files of the jars: the jar of each class by internal name, and each class read
/// once.
#[derive(Default)]
pub struct JavaClasses {
    jars: FxHashMap<String, usize>,
    read: FxHashMap<String, Option<Rc<ClassInfo>>>,
}

impl Universe {
    /// Makes the Java classes of the class files of the jars of indices `read` known and enters
    /// their packages: those of no TASTy file, outside `java.*` and `javax.*`. The library's
    /// runtime stubs of these classes are dropped.
    pub fn use_java_classfiles(&mut self, read: &[usize]) -> std::io::Result<()> {
        let mut jars = self.library.java_classes(read)?;
        jars.retain(|name, _| !name.starts_with("java/") && !name.starts_with("javax/"));
        let packages: FxHashSet<String> =
            jars.keys().filter_map(|n| n.rsplit_once('/')).map(|(p, _)| p.replace('/', ".")).collect();
        for p in packages {
            self.enter_package(&p);
        }
        let stubs: Vec<SymbolId> = self.java_stubs.iter().copied().collect();
        for s in stubs {
            let full = self.full_name(s).replace('.', "/");
            if jars.contains_key(full.strip_suffix('$').unwrap_or(&full)) {
                self.unenter(s);
                self.java_stubs.remove(&s);
            }
        }
        self.java_classes = Some(JavaClasses { jars, read: FxHashMap::default() });
        Ok(())
    }

    /// Enters class `name` of package `pkg` (an object `C$` stands for its class `C`) from
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
        if info.access & ACC_MODULE != 0 {
            return Vec::new();
        }
        let entered = enter(self, pkg, class, &info, info.access);
        entered.into_iter().filter(|&d| self.symbol(d).name == name).collect()
    }
}

/// Enters class `info` of Java access flags `access` into `owner` as `name` with its nested
/// classes, the static ones into its object's class, the others into it (dotty's
/// `enterOwnInnerClasses`): the class, its object's class and its object, none if its signature
/// is refused.
fn enter(u: &mut Universe, owner: SymbolId, name: &str, info: &ClassInfo, access: u16) -> Vec<SymbolId> {
    let Some(entered) = class::enter(u, owner, name, info, access) else { return Vec::new() };
    for nested in &info.inner_classes {
        let (Some(outer), Some(simple)) = (&nested.outer, &nested.name) else { continue };
        if *outer != info.name {
            continue;
        }
        let Some(inner) = java_class(u, &nested.inner) else { continue };
        let owner = if nested.flags & ACC_STATIC != 0 { entered[1] } else { entered[0] };
        let access = inner.access & !(ACC_PUBLIC | ACC_PROTECTED | ACC_PRIVATE) | nested.flags;
        enter(u, owner, simple, &inner, access);
    }
    entered
}

/// Reads the class of internal name `internal` from the jars' class files; none if they lack it,
/// it is no class file or one compiled from Scala.
pub fn java_class(u: &mut Universe, internal: &str) -> Option<Rc<ClassInfo>> {
    let classes = u.java_classes.as_mut()?;
    if let Some(info) = classes.read.get(internal) {
        return info.clone();
    }
    let jar = *classes.jars.get(internal)?;
    let info = u.library.read_class(jar, internal).ok().and_then(|b| classfile::read(&b).ok());
    let info = info.filter(|i| !i.scala).map(Rc::new);
    u.java_classes.as_mut()?.read.insert(internal.to_owned(), info.clone());
    info
}
