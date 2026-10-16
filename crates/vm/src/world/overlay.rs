//! The library overlay (ADR 0009): Scala sources in `overlay/src`, compiled into `overlay/nir`
//! and embedded, whose methods replace or add to the library's member by member.

use super::files::File;
use super::{LoadError, World};
use nir::{Caches, Defn, DefnKind, Global, GlobalId, Inst, Op, Pool, Reader};
use std::rc::Rc;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/overlay.rs"));
}

/// Members the overlay adds to library classes, one `top-level signature` per line.
const ADDED: &str = include_str!("../../../../overlay/added.txt");

/// The signature of `Predef.???`, whose call marks a stub.
const STUB: &str = "D18$qmark$qmark$qmarknEO";

/// The overlay file of top-level `name`.
pub fn file(name: &str) -> Option<&'static [u8]> {
    embedded::FILES.iter().find(|(top, _)| *top == name).map(|(_, data)| *data)
}

fn added(top: &str, sig: &str) -> bool {
    ADDED.lines().any(|l| l.split_once(' ') == Some((top, sig)))
}

/// What an overlay member stands for.
enum Role {
    /// Declared for the compiler: a field or a stub, which the library must have.
    Library,
    /// A constructor or an abstract method, left to the library.
    Ignored,
    /// A method replacing the library's, or added to it.
    Replacement,
}

/// Whether `insts` only call `???`: they call it and never return.
fn is_stub(pool: &Pool, insts: &[Inst]) -> bool {
    let calls =
        |i: &Inst| matches!(i, Inst::Let { op: Op::Method { sig, .. }, .. } if pool.str(*sig) == STUB);
    insts.iter().any(calls) && !insts.iter().any(|i| matches!(i, Inst::Ret(_)))
}

fn role(pool: &Pool, defn: &Defn, sig: &str) -> Role {
    match &defn.kind {
        DefnKind::Var { .. } | DefnKind::Const { .. } => Role::Library,
        DefnKind::Define { .. } if sig.starts_with('R') => Role::Ignored,
        DefnKind::Define { body, .. } if is_stub(pool, &body.insts) => Role::Library,
        DefnKind::Define { .. } => Role::Replacement,
        _ => Role::Ignored,
    }
}

impl World {
    /// Places the members of overlay file `data` over those of the library's file of `top`,
    /// which is loaded; on a mismatch, nothing, and `top` stays unusable.
    pub fn merge(&mut self, top: GlobalId, data: &'static [u8]) -> Result<(), LoadError> {
        let merged = self.try_merge(top, data);
        if let Err(e) = &merged {
            let Global::Top(name) = *self.pool.global(top) else { return merged };
            self.broken.insert(name, e.to_string());
        }
        merged
    }

    fn try_merge(&mut self, top: GlobalId, data: &'static [u8]) -> Result<(), LoadError> {
        let name = self.pool.top_name(top).to_owned();
        let corrupt = |e| LoadError::Corrupt(format!("{name} (overlay)"), e);
        let mut caches = Caches::default();
        let index =
            Reader::new(data, &mut caches, &mut self.pool).and_then(|mut r| r.index()).map_err(corrupt)?;
        let members: Vec<_> =
            index.into_iter().filter(|(g, _)| matches!(self.pool.global(*g), Global::Member(..))).collect();
        let mut reader = Reader::resume(data, &mut caches, &mut self.pool).map_err(corrupt)?;
        reader.debug_info = false;
        let defns = members
            .into_iter()
            .map(|(g, offset)| Ok((g, offset, reader.defn_at(offset)?)))
            .collect::<nir::Result<Vec<_>>>()
            .map_err(corrupt)?;
        let mismatch = |sig: &str, what: &str| Err(LoadError::Overlay(format!("{name} {sig}: {what}")));
        let mut replacements = Vec::new();
        for (g, offset, defn) in defns {
            let Global::Member(owner, sig) = *self.pool.global(g) else { continue };
            let sig = self.pool.str(sig).to_owned();
            if owner != top {
                return mismatch(&sig, "a member of another class");
            }
            let in_library = self.place(g)?.is_some();
            match role(&self.pool, &defn, &sig) {
                Role::Ignored => {}
                Role::Library if in_library => {}
                Role::Library => return mismatch(&sig, "not in the library"),
                Role::Replacement if !in_library && !added(&name, &sig) => {
                    return mismatch(&sig, "added, not in overlay/added.txt");
                }
                Role::Replacement => replacements.push((g, offset, defn, in_library)),
            }
        }
        let file = self.files.len() as u32;
        self.files.push(File { data: std::borrow::Cow::Borrowed(data), caches, table: None });
        for (g, offset, defn, in_library) in replacements {
            if !in_library {
                self.members(top)?;
                self.members.entry(top).or_default().push(g);
            }
            self.places.insert(g, (file, offset));
            self.defns.insert(g, Rc::new(defn));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classpath::Classpath;
    use std::path::Path;

    #[test]
    fn overlay_matches_library() {
        let libs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs");
        let Ok(entries) = std::fs::read_dir(&libs) else {
            assert!(std::env::var_os("CI").is_none(), "ref/libs missing: run `just fetch-libs`");
            return;
        };
        let jars: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "jar"))
            .collect();
        let mut library = Classpath::open(&jars).unwrap();
        let mut world = World::new(Classpath::open(&jars).unwrap());
        for (top, _) in embedded::FILES {
            let helper = top.starts_with("scalai.") || top.starts_with("scala.quoted.runtime.impl.");
            assert!(
                library.find(top).is_some() || helper,
                "{top}: neither in the library nor a helper in package scalai or scala.quoted.runtime.impl"
            );
            let g = world.top(top);
            world.members(g).unwrap_or_else(|e| panic!("{e}"));
        }
        for (top, sig) in ADDED.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split_once(' ')) {
            let (data, _) =
                library.find(top).unwrap_or_else(|| panic!("overlay/added.txt: no library class {top}"));
            let table = crate::member_table::build(&data, top).expect("a member table");
            let table = crate::member_table::Table(Box::leak(table.into_boxed_slice()));
            assert!(table.find(sig).is_none(), "overlay/added.txt: {top} {sig} is in the library");
            let owner = world.top(top);
            let g = world.member(owner, sig);
            assert!(
                world.place(g).unwrap().is_some(),
                "overlay/added.txt: {top} {sig} is not in the overlay"
            );
        }
    }
}
