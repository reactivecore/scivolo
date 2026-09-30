//! Erased signatures computed from TASTy against the mangled names Scala Native's compiler gave
//! the same members in the NIR of its `scalalib`, for every class of the standard library.

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

/// Every class of the universe, nested ones included.
fn all_classes(u: &mut Universe) -> Vec<SymbolId> {
    let (mut out, mut stack) = (Vec::new(), vec![u.root()]);
    while let Some(owner) = stack.pop() {
        for d in u.decls(owner).unwrap() {
            match u.symbol(d).kind {
                Kind::Package => stack.push(d),
                Kind::Class => {
                    out.push(d);
                    stack.push(d);
                }
                _ => {}
            }
        }
    }
    out
}

/// `D<length><name>`, the start of the signatures of methods named `name`.
fn method_prefix(name: &str) -> String {
    format!("D{}{name}", name.encode_utf16().count())
}

#[test]
fn signatures_match_scalalib_nir() {
    let Some((mut u, mut zip)) = libs() else { return };
    let (mut matched, mut total, mut misses, mut skipped) = (0, 0, Vec::new(), Vec::new());
    let (mut found, mut wanted) = (0, 0);
    for class in all_classes(&mut u) {
        let binary = u.binary_name(class);
        let Some(nir) = nir_methods(&mut zip, &binary) else {
            skipped.push(binary);
            continue;
        };
        let mut ours = HashSet::new();
        let mut names = HashSet::new();
        for m in u.symbol(class).decls.clone() {
            let Some(sig) = u.method_signature(m) else { continue };
            let s = u.symbol(m);
            names.insert(method_prefix(&tasty::encode(s.target.as_deref().unwrap_or(&s.name))));
            total += 1;
            if nir.contains(&sig) {
                matched += 1;
            } else {
                misses.push(format!("{binary}: {sig}"));
            }
            ours.insert(sig);
        }
        for sig in nir.iter().filter(|s| names.iter().any(|n| s.starts_with(n.as_str()))) {
            wanted += 1;
            found += usize::from(ours.contains(sig));
        }
    }
    assert!(skipped.is_empty(), "classes without NIR: {skipped:?}");
    let (precision, recall) = (matched as f64 / total as f64, found as f64 / wanted as f64);
    assert!(
        total > 13_000 && precision >= 0.997 && recall >= 0.875,
        "precision {matched}/{total}, recall {found}/{wanted}; misses:\n{}",
        misses.join("\n")
    );
}
