//! Finds the entry method among the program's own NIR files.

use crate::R;
use nir::{DefnKind, GlobalId, Pool, Type, TypeId};
use std::path::{Path, PathBuf};
use vm::World;

fn nir_files(dir: &Path) -> R<Vec<PathBuf>> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut files: Vec<PathBuf> =
        entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "nir")).collect();
    files.sort();
    Ok(files)
}

/// The one `(Int) => Int` method named `name` defined in `dirs`, as a global of `world`.
pub fn find(dirs: &[PathBuf], name: &str, world: &mut World) -> R<GlobalId> {
    let mut pool = Pool::default();
    let mut caches = nir::Caches::default();
    let prefix = format!("D{}{name}", name.len());
    let mut found = Vec::new();
    for dir in dirs {
        for file in nir_files(dir)? {
            let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let what = |e: nir::Error| format!("{}: {e}", file.display());
            let mut reader = nir::Reader::new(&bytes, &mut caches, &mut pool).map_err(what)?;
            reader.debug_info = false;
            for d in reader.all().map_err(what)? {
                let DefnKind::Define { ty, .. } = d.kind else { continue };
                let int = |t: &TypeId| matches!(pool.ty(*t), Type::Int);
                let int_to_int =
                    matches!(pool.ty(ty), Type::Function(ps, r) if ps.len() == 1 && int(&ps[0]) && int(r));
                if let Some(sig) = pool.member_sig(d.name).filter(|s| s.starts_with(&prefix) && int_to_int) {
                    found.push((pool.top_name(d.name).to_owned(), sig.to_owned()));
                }
            }
        }
    }
    match &found[..] {
        [(top, sig)] => {
            let top = world.top(top);
            Ok(world.member(top, sig))
        }
        [] => Err(format!("no method `{name}` of type (Int) => Int")),
        _ => {
            let names: Vec<String> = found.iter().map(|(top, sig)| format!("{top}::{sig}")).collect();
            Err(format!("several methods `{name}` of type (Int) => Int: {}", names.join(", ")))
        }
    }
}
