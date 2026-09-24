use std::time::{Duration, Instant};
use tasty_reader::{Ast, NodeId, Tasty, View, tags, view};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MethodId(pub u32);

pub struct File {
    pub tasty: Tasty<'static>,
    pub ast: Ast,
}

pub struct Class {
    pub name: String,
    pub file: u32,
    pub node: NodeId,
}

pub struct Program {
    pub files: Vec<File>,
    pub classes: Vec<Class>,
}

pub struct LoadTimes {
    pub read: Duration,
    pub parse: Duration,
    pub index: Duration,
}

impl Program {
    pub fn load(paths: &[String]) -> Result<(Program, LoadTimes), String> {
        let t0 = Instant::now();
        let mut raw = Vec::new();
        for p in paths {
            let bytes = std::fs::read(p).map_err(|e| format!("{p}: {e}"))?;
            raw.push(&*Box::leak(bytes.into_boxed_slice()));
        }
        let t1 = Instant::now();
        let mut files = Vec::new();
        for bytes in raw {
            let tasty = Tasty::parse(bytes).map_err(|e| e.to_string())?;
            let sec = tasty.section("ASTs").ok_or("no ASTs section")?;
            let ast = Ast::parse(sec.bytes).map_err(|e| e.to_string())?;
            files.push(File { tasty, ast });
        }
        let t2 = Instant::now();
        let mut prog = Program { files, classes: Vec::new() };
        prog.index();
        let times = LoadTimes { read: t1 - t0, parse: t2 - t1, index: t2.elapsed() };
        Ok((prog, times))
    }

    pub fn class(&self, id: ClassId) -> &Class {
        &self.classes[id.0 as usize]
    }

    fn index(&mut self) {
        for (fi, f) in self.files.iter().enumerate() {
            for &root in f.ast.roots() {
                let View::Package { stats, .. } = view(&f.ast, root) else { continue };
                for &s in stats {
                    if let View::TypeDef { name, is_class: true, .. } = view(&f.ast, s) {
                        let name = f.tasty.names.show(name);
                        self.classes.push(Class { name, file: fi as u32, node: s });
                    }
                }
            }
        }
    }

    /// `(class, method)` names of every `@main` def or object `main` method.
    pub fn entries(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for c in &self.classes {
            let f = &self.files[c.file as usize];
            let View::TypeDef { rhs, .. } = view(&f.ast, c.node) else { continue };
            let View::Template(parts) = view(&f.ast, rhs) else { continue };
            for &d in parts.body {
                let View::DefDef { name, parts: dp } = view(&f.ast, d) else { continue };
                let name = f.tasty.names.show(name);
                let annotated = dp.mods.iter().any(|&m| f.ast.tag(m) == tags::ANNOTATION);
                if annotated || (name == "main" && c.name.ends_with('$')) {
                    out.push((c.name.clone(), name));
                }
            }
        }
        out
    }
}
