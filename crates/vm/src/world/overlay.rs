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

fn role(pool: &Pool, defn: &Defn, sig: &str) -> Role {
    let calls_stub = |insts: &[Inst]| {
        insts
            .iter()
            .any(|i| matches!(i, Inst::Let { op: Op::Method { sig, .. }, .. } if pool.str(*sig) == STUB))
    };
    match &defn.kind {
        DefnKind::Var { .. } | DefnKind::Const { .. } => Role::Library,
        DefnKind::Define { .. } if sig.starts_with('R') => Role::Ignored,
        DefnKind::Define { body, .. } if calls_stub(&body.insts) => Role::Library,
        DefnKind::Define { .. } => Role::Replacement,
        _ => Role::Ignored,
    }
}

impl World {
    /// Places the members of overlay file `data` over those of the library's file of `top`,
    /// which is loaded.
    pub fn merge(&mut self, top: GlobalId, data: &'static [u8]) -> Result<(), LoadError> {
        let name = self.pool.top_name(top).to_owned();
        let corrupt = |e| LoadError::Corrupt(name.clone(), e);
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
        let file = self.files.len() as u32;
        self.files.push(File { data: std::borrow::Cow::Borrowed(data), caches, table: None });
        for (g, offset, defn) in defns {
            let Global::Member(_, sig) = *self.pool.global(g) else { continue };
            let sig = self.pool.str(sig).to_owned();
            let in_library = self.place(g)?.is_some();
            match role(&self.pool, &defn, &sig) {
                Role::Ignored => {}
                Role::Library if in_library => {}
                Role::Library => return Err(LoadError::Overlay(format!("{name} {sig}: not in the library"))),
                Role::Replacement => {
                    if !in_library {
                        if !added(&name, &sig) {
                            return Err(LoadError::Overlay(format!(
                                "{name} {sig}: added, not in overlay/added.txt"
                            )));
                        }
                        self.members(top)?;
                        self.members.entry(top).or_default().push(g);
                    }
                    self.places.insert(g, (file, offset));
                    self.defns.insert(g, Rc::new(defn));
                }
            }
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
        let mut world = World::new(Classpath::open(&jars).unwrap());
        assert!(!embedded::FILES.is_empty());
        for (top, _) in embedded::FILES {
            let g = world.top(top);
            world.defn(g).unwrap_or_else(|e| panic!("{e}"));
        }
        for (top, sig) in ADDED.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split_once(' ')) {
            let owner = world.top(top);
            let g = world.member(owner, sig);
            assert!(
                world.place(g).unwrap().is_some(),
                "overlay/added.txt: {top} {sig} is not in the overlay"
            );
        }
    }
}
