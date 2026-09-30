//! Erased signatures computed from TASTy against the mangled names Scala Native's compiler gave
//! the same members in the NIR of its `scalalib`.

use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use tasty::{Kind, SymbolId, Universe};

fn libs() -> Option<(Universe, zip::ZipArchive<std::fs::File>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs");
    let (tasty, nir) =
        (dir.join("scala-library-3.9.0.jar"), dir.join("scalalib_native0.5_3-3.9.0%2B0.5.12.jar"));
    if !tasty.exists() || !nir.exists() {
        assert!(std::env::var_os("CI").is_none(), "{} missing: run `just fetch-libs`", dir.display());
        return None;
    }
    let zip = zip::ZipArchive::new(std::fs::File::open(nir).unwrap()).unwrap();
    Some((Universe::open(&[tasty]).unwrap(), zip))
}

/// The public instance method signatures of the class file `binary` in the NIR jar.
fn nir_methods(zip: &mut zip::ZipArchive<std::fs::File>, binary: &str) -> Option<HashSet<String>> {
    let mut file = zip.by_name(&format!("{}.nir", binary.replace('.', "/"))).ok()?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).unwrap();
    let (mut caches, mut pool) = (nir::Caches::default(), nir::Pool::default());
    let index = nir::Reader::new(&data, &mut caches, &mut pool).unwrap().index().unwrap();
    let sigs = index
        .iter()
        .filter_map(|(g, _)| pool.member_sig(*g))
        .filter(|s| s.starts_with('D') && s.ends_with("EO"));
    Some(sigs.map(str::to_owned).collect())
}

fn classes(u: &mut Universe, pkg: &str) -> Vec<SymbolId> {
    let p = u.package(pkg).unwrap();
    u.decls(p).unwrap().into_iter().filter(|&c| u.symbol(c).kind == Kind::Class).collect()
}

#[test]
fn signatures_match_scalalib_nir() {
    let Some((mut u, mut zip)) = libs() else { return };
    let (mut matched, mut total, mut misses) = (0, 0, Vec::new());
    for pkg in ["scala", "scala.collection.immutable", "scala.collection", "scala.util"] {
        for class in classes(&mut u, pkg) {
            let binary = u.binary_name(class);
            let Some(nir) = nir_methods(&mut zip, &binary) else { continue };
            for m in u.symbol(class).decls.clone() {
                let Some(sig) = u.method_signature(m) else { continue };
                total += 1;
                if nir.contains(&sig) {
                    matched += 1;
                } else {
                    let name = format!(
                        "D{}{}",
                        tasty::encode(&u.symbol(m).name).encode_utf16().count(),
                        tasty::encode(&u.symbol(m).name)
                    );
                    let near: Vec<&String> = nir.iter().filter(|s| s.starts_with(&name)).take(2).collect();
                    misses.push(format!("{binary}: {sig}  nir: {near:?}"));
                }
            }
        }
    }
    let share = matched as f64 / total as f64;
    assert!(total > 4000 && share >= 0.98, "matched {matched} of {total}:\n{}", misses.join("\n"));
}
