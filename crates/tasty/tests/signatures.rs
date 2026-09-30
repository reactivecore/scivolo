//! Erased signatures computed from TASTy, bridges included, against the mangled names Scala
//! Native's compiler gave the same members in its NIR: every class of the standard library
//! against `scalalib`, every JDK class of javalib against `javalib`.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use tasty::{Kind, SymbolId, Universe};

type Zip = zip::ZipArchive<std::fs::File>;

/// The jars of `ref/libs` named `names`, if all are there; in CI, all but javalib's TASTy must be.
fn jars(names: &[&str]) -> Option<Vec<PathBuf>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ref/libs");
    let jars: Vec<PathBuf> = names.iter().map(|n| dir.join(n)).collect();
    if let Some(missing) = jars.iter().find(|j| !j.exists()) {
        let built = missing.ends_with(JAVALIB_TASTY);
        let hint = "run `just fetch-libs` and `just javalib-tasty`";
        assert!(built || std::env::var_os("CI").is_none(), "{} missing: {hint}", missing.display());
        eprintln!("skipped: {} missing ({hint})", missing.display());
        return None;
    }
    Some(jars)
}

const JAVALIB_TASTY: &str = "javalib-tasty-0.5.12.jar";

fn open_zip(path: &Path) -> Zip {
    zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap()
}

/// The public instance method signatures of the class file `binary` in the NIR jar.
fn nir_methods(zip: &mut Zip, binary: &str) -> Option<HashSet<String>> {
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

/// Every class in `owner`, nested ones included.
fn classes_in(u: &mut Universe, owner: SymbolId) -> Vec<SymbolId> {
    let (mut out, mut stack) = (Vec::new(), vec![owner]);
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

#[derive(Default)]
struct Stats {
    matched: usize,
    total: usize,
    bridges_matched: usize,
    bridges: usize,
    found: usize,
    wanted: usize,
    misses: Vec<String>,
    skipped: Vec<String>,
}

impl Stats {
    /// The comparison of `classes` with their NIR in the first of `zips` that has it.
    fn of(u: &mut Universe, zips: &mut [Zip], classes: Vec<SymbolId>) -> Stats {
        let mut s = Stats::default();
        for class in classes {
            let binary = u.binary_name(class);
            let Some(nir) = zips.iter_mut().find_map(|z| nir_methods(z, &binary)) else {
                s.skipped.push(binary);
                continue;
            };
            let (mut ours, mut names) = (HashSet::new(), HashSet::new());
            for m in u.symbol(class).decls.clone() {
                let Some(sig) = u.method_signature(m) else { continue };
                let sym = u.symbol(m);
                names.insert(method_prefix(&tasty::encode(sym.target.as_deref().unwrap_or(&sym.name))));
                s.total += 1;
                s.matched += usize::from(nir.contains(&sig));
                if !nir.contains(&sig) {
                    s.misses.push(format!("{binary}: {sig}"));
                }
                ours.insert(sig);
                for bridge in u.bridge_signatures(m) {
                    s.bridges += 1;
                    s.bridges_matched += usize::from(nir.contains(&bridge));
                    if !nir.contains(&bridge) {
                        s.misses.push(format!("{binary}: bridge {bridge}"));
                    }
                    ours.insert(bridge);
                }
            }
            for sig in nir.iter().filter(|sig| names.iter().any(|n| sig.starts_with(n.as_str()))) {
                s.wanted += 1;
                s.found += usize::from(ours.contains(sig));
            }
        }
        s
    }

    fn check(&self, total: usize, bridges: usize, precision: f64, recall: f64) {
        let ratio = |a: usize, b: usize| a as f64 / b.max(1) as f64;
        let numbers = format!(
            "precision {}/{}, bridges {}/{}, recall {}/{}",
            self.matched, self.total, self.bridges_matched, self.bridges, self.found, self.wanted
        );
        eprintln!("{numbers}");
        assert!(
            self.total > total
                && self.bridges > bridges
                && ratio(self.matched, self.total) >= precision
                && ratio(self.bridges_matched, self.bridges) >= precision
                && ratio(self.found, self.wanted) >= recall,
            "{numbers}; misses:\n{}",
            self.misses.join("\n")
        );
    }
}

#[test]
fn signatures_match_scalalib_nir() {
    let Some(jars) = jars(&["scala-library-3.9.0.jar", "scalalib_native0.5_3-3.9.0%2B0.5.12.jar"]) else {
        return;
    };
    let mut u = Universe::open(&jars[..1]).unwrap();
    let root = u.root();
    let classes = classes_in(&mut u, root);
    let stats = Stats::of(&mut u, &mut [open_zip(&jars[1])], classes);
    assert!(stats.skipped.is_empty(), "classes without NIR: {:?}", stats.skipped);
    stats.check(13_000, 1_500, 0.997, 0.98);
}

#[test]
fn signatures_match_javalib_nir() {
    let names = [
        "javalib_native0.5_3-0.5.12.jar",
        "scala-library-3.9.0.jar",
        JAVALIB_TASTY,
        "nativelib_native0.5_3-0.5.12.jar",
        "clib_native0.5_3-0.5.12.jar",
        "posixlib_native0.5_3-0.5.12.jar",
    ];
    let Some(jars) = jars(&names) else { return };
    let mut u = Universe::open(&jars[1..]).unwrap();
    let java = u.package("java").unwrap();
    let classes = classes_in(&mut u, java);
    let stats = Stats::of(&mut u, &mut [open_zip(&jars[0]), open_zip(&jars[3])], classes);
    assert!(stats.skipped.is_empty(), "classes without NIR: {:?}", stats.skipped);
    stats.check(10_000, 500, 0.999, 0.985);
}
