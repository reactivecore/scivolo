//! The Java classes of the jars' class files, entered on lookup with their nested classes
//! (ADR 0117).

use super::super::{SymbolId, Universe, module_class_name};
use super::class;
use super::java_sources::JavaSources;
use crate::types::NameId;
use classfile::{ACC_MODULE, ACC_PRIVATE, ACC_PROTECTED, ACC_PUBLIC, ACC_STATIC, ClassInfo};
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

/// The Java class files of the jars and the Java sources: the jar of each class by internal
/// name, each class read once, the refusals of the classes read but refused (Scala 2 class files,
/// Java sources' classes of a name not found), and the lookups that found one.
#[derive(Default)]
pub struct JavaClasses {
    jars: FxHashMap<String, usize>,
    sources: JavaSources,
    read: FxHashMap<String, Option<Rc<ClassInfo>>>,
    refused: FxHashMap<String, String>,
    refused_lookups: RefusedLookups,
}

/// The lookups that found a refused class: how many, and the last one's refusal.
#[derive(Default)]
struct RefusedLookups {
    count: usize,
    last: String,
}

impl JavaClasses {
    /// Determines whether a jar holds class `internal`.
    pub fn in_jars(&self, internal: &str) -> bool {
        self.jars.contains_key(internal)
    }

    /// Determines whether a jar's class file or a Java source declares class `internal`.
    pub fn declares(&self, internal: &str) -> bool {
        self.in_jars(internal) || self.sources.declares(internal)
    }
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
        self.java_classes = Some(JavaClasses { jars, ..JavaClasses::default() });
        self.class_file_misses.clear();
        Ok(())
    }

    /// Makes the classes of Java sources `sources` known and enters their packages; they shadow
    /// the jars' classes of the same names. Needs `use_java_classfiles` first.
    pub fn use_java_sources(&mut self, sources: JavaSources) {
        for p in sources.packages() {
            self.enter_package(&p);
        }
        if let Some(classes) = self.java_classes.as_mut() {
            classes.sources = sources;
        }
        self.class_file_misses.clear();
    }

    /// The Java sources `use_java_sources` made known, if any.
    pub fn java_sources(&self) -> Option<JavaSources> {
        self.java_classes.as_ref().map(|c| c.sources.clone())
    }

    /// Counts the lookups that found a refused class: a jar's Scala 2 class file, whose pickle
    /// this reader does not read (a class `p.C`, or the package object `p.package` of a name `p`
    /// lacks), or a Java source's class of a name not found.
    pub fn refused_lookups(&self) -> usize {
        self.java_classes.as_ref().map_or(0, |c| c.refused_lookups.count)
    }

    /// The refusal of the last lookup `refused_lookups` counts.
    pub fn last_refused_lookup(&self) -> Option<&str> {
        let last = &self.java_classes.as_ref()?.refused_lookups.last;
        (!last.is_empty()).then_some(last.as_str())
    }

    /// Counts a lookup of the package object of package `pkg` if a jar holds it as a Scala 2 class
    /// file.
    pub fn look_up_scala2_package_object(&mut self, pkg: SymbolId) {
        let outer = self.full_name(pkg).replace('.', "/");
        if !outer.is_empty() {
            java_class(self, &format!("{outer}/package"));
        }
    }

    /// Reads the class of internal name `internal` from the Java sources, the jars' class files,
    /// else the JDK.
    pub fn java_class_info(&mut self, internal: &str) -> Option<Rc<ClassInfo>> {
        java_class(self, internal).or_else(|| self.jdk.as_mut()?.class(internal).cloned())
    }

    /// Whether the universe uses the jars' Java class files, as a compile for the JVM does.
    pub fn uses_java_classfiles(&self) -> bool {
        self.java_classes.is_some()
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
        let Some(info) = java_class(self, &internal) else {
            self.look_up_scala2_package_object(pkg);
            return Vec::new();
        };
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
/// is refused. A nested class is read from the jars, else from the JDK.
pub fn enter(u: &mut Universe, owner: SymbolId, name: &str, info: &ClassInfo, access: u16) -> Vec<SymbolId> {
    let Some(entered) = class::enter(u, owner, name, info, access) else { return Vec::new() };
    for nested in &info.inner_classes {
        let (Some(outer), Some(simple)) = (&nested.outer, &nested.name) else { continue };
        if *outer != info.name {
            continue;
        }
        let inner = java_class(u, &nested.inner).or_else(|| u.jdk.as_mut()?.class(&nested.inner).cloned());
        let Some(inner) = inner else { continue };
        let owner = if nested.flags & ACC_STATIC != 0 { entered[1] } else { entered[0] };
        let access = inner.access & !(ACC_PUBLIC | ACC_PROTECTED | ACC_PRIVATE) | nested.flags;
        enter(u, owner, simple, &inner, access);
    }
    entered
}

/// Reads the class of internal name `internal` from the Java sources, else the jars' class files;
/// none if they lack it, it is no class file, or it is refused, which counts as a refused lookup:
/// a class file compiled from Scala, or a Java source's class of a name not found.
pub fn java_class(u: &mut Universe, internal: &str) -> Option<Rc<ClassInfo>> {
    let classes = u.java_classes.as_mut()?;
    if !classes.read.contains_key(internal) {
        let info = if classes.sources.declares(internal) {
            let sources = classes.sources.clone();
            classes.read.insert(internal.to_owned(), None);
            let info = sources.class_info(u, internal);
            info.map(Some).map_err(|e| format!("Java source class `{}`: {e}", dotted(internal)))
        } else {
            let jar = *classes.jars.get(internal)?;
            let info = u.library.read_class(jar, internal).ok().and_then(|b| classfile::read(&b).ok());
            match info {
                Some(i) if i.scala => Err(format!("Scala 2 class file `{}` of a jar", dotted(internal))),
                info => Ok(info),
            }
        };
        let classes = u.java_classes.as_mut()?;
        let info = info.unwrap_or_else(|e| {
            classes.refused.insert(internal.to_owned(), e);
            None
        });
        classes.read.insert(internal.to_owned(), info.map(Rc::new));
    }
    let classes = u.java_classes.as_mut()?;
    if let Some(refusal) = classes.refused.get(internal) {
        classes.refused_lookups.count += 1;
        classes.refused_lookups.last.clone_from(refusal);
    }
    classes.read[internal].clone()
}

/// Internal name `internal` with dots for its slashes.
fn dotted(internal: &str) -> String {
    internal.replace('/', ".")
}
