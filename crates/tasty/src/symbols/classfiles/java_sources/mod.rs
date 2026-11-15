//! Java sources compiled with Scala ones (ADR 0124): parsed for their declarations (dotty's
//! `JavaParsers`), each class turned into the class file javac would write for its signatures and
//! entered on lookup as a jar's class is.

mod annotations;
mod class_info;
mod enum_constants;
mod fields;
mod literals;
mod members;
mod parser;
mod permitted;
mod records;
mod resolve;
mod scanner;
mod syntax;
mod text_blocks;
mod type_signatures;
mod written_types;

use super::super::Universe;
use classfile::ClassInfo;
use rustc_hash::FxHashMap;
use std::rc::Rc;
use syntax::{Class, Unit};

/// The Java sources of a run: their units, and where each class is by internal name.
#[derive(Clone, Default)]
pub struct JavaSources {
    units: Vec<Rc<Unit>>,
    classes: Rc<FxHashMap<String, (usize, Vec<usize>)>>,
}

/// A class of a Java source: its unit, the indices of it and its outer classes from the top
/// level, its internal name.
#[derive(Clone)]
pub struct Located {
    pub unit: Rc<Unit>,
    path: Vec<usize>,
    pub internal: String,
}

impl Located {
    /// The class declared here.
    pub fn class(&self) -> &Class {
        let top = &self.unit.classes[self.path[0]];
        self.path[1..].iter().fold(top, |c, &i| &c.nested[i])
    }

    /// The class this one is nested in, if any.
    pub fn outer(&self) -> Option<Located> {
        let (_, outer) = self.path.split_last()?;
        if outer.is_empty() {
            return None;
        }
        let (internal, _) = self.internal.rsplit_once('$')?;
        Some(Located { unit: self.unit.clone(), path: outer.to_vec(), internal: internal.to_owned() })
    }

    /// The internal names of this class and the classes it is nested in, innermost first.
    pub fn enclosing(&self) -> Vec<String> {
        std::iter::successors(Some(self.clone()), Located::outer).map(|l| l.internal).collect()
    }
}

impl JavaSources {
    /// Parses Java sources `files` (name and text); the refusal of the first file this parser does
    /// not read or of a class two files declare.
    pub fn parse(files: &[(String, String)]) -> Result<JavaSources, String> {
        let mut units = Vec::new();
        let mut classes = FxHashMap::default();
        for (name, text) in files {
            let unit =
                parser::parse(text).map_err(|e| format!("{name}: the Java parser does not support {e}"))?;
            let package = unit.package.replace('.', "/");
            for (i, c) in unit.classes.iter().enumerate() {
                let internal =
                    if package.is_empty() { c.name.clone() } else { format!("{package}/{}", c.name) };
                if classes.contains_key(&internal) {
                    return Err(format!("{name}: Java class `{internal}` declared twice"));
                }
                enter(&mut classes, units.len(), vec![i], internal, c);
            }
            units.push(Rc::new(unit));
        }
        Ok(JavaSources { units, classes: Rc::new(classes) })
    }

    /// The dotted names of the sources' packages.
    pub fn packages(&self) -> Vec<String> {
        self.units.iter().map(|u| u.package.clone()).filter(|p| !p.is_empty()).collect()
    }

    /// Determines whether the sources declare class `internal`.
    pub fn declares(&self, internal: &str) -> bool {
        self.classes.contains_key(internal)
    }

    /// Where class `internal` is, if the sources declare it.
    pub fn located(&self, internal: &str) -> Option<Located> {
        let (unit, path) = self.classes.get(internal)?;
        let (unit, path, internal) = (self.units[*unit].clone(), path.clone(), internal.to_owned());
        Some(Located { unit, path, internal })
    }

    /// The classes of unit `unit`, nested ones included, by internal name.
    pub fn unit_classes(&self, unit: &Rc<Unit>) -> Vec<Located> {
        let mut names: Vec<&String> = self.classes.keys().collect();
        names.sort();
        names.into_iter().filter_map(|n| self.located(n)).filter(|l| Rc::ptr_eq(&l.unit, unit)).collect()
    }

    /// Builds the class file of class `internal` of the sources; the refusal of a name not found.
    pub fn class_info(&self, u: &mut Universe, internal: &str) -> Result<ClassInfo, String> {
        let at = self.located(internal).ok_or_else(|| format!("Java class `{internal}`"))?;
        class_info::class_info(u, self, &at)
    }
}

/// Records class `c` at `path` of unit `unit` as `internal`, with its nested classes.
fn enter(
    classes: &mut FxHashMap<String, (usize, Vec<usize>)>,
    unit: usize,
    path: Vec<usize>,
    internal: String,
    c: &Class,
) {
    for (i, n) in c.nested.iter().enumerate() {
        let mut nested = path.clone();
        nested.push(i);
        enter(classes, unit, nested, format!("{internal}${}", n.name), n);
    }
    classes.insert(internal, (unit, path));
}
