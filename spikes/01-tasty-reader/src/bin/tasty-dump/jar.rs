use std::collections::BTreeMap;
use std::io::Read;
use std::time::{Duration, Instant};
use tasty_reader::{Ast, Tasty, tags};

struct Entry {
    name: String,
    bytes: Vec<u8>,
}

pub fn run(path: &str, args: &[String]) -> Result<(), String> {
    let flag = |f: &str| args.iter().position(|a| a == f).and_then(|i| args.get(i + 1));
    let repeat: u32 = flag("--repeat").map_or(Ok(5), |v| v.parse()).map_err(|e| format!("--repeat: {e}"))?;

    let (mut entries, mut zip_entries) = (Vec::new(), 0);
    let t_dir = best(repeat, || zip_entries = open(path).map_or(0, |z| z.len()));
    let t_unzip = best(repeat, || entries = load(path).unwrap_or_default());
    if entries.is_empty() {
        load(path)?;
    }

    if let Some(pat) = flag("--dump") {
        let e = entries.iter().find(|e| e.name.contains(pat.as_str())).ok_or("no matching entry")?;
        println!("== {}", e.name);
        return crate::dump_file(&e.bytes, args.contains(&"--names".into()));
    }

    let total: usize = entries.iter().map(|e| e.bytes.len()).sum();
    let mut failures = Vec::new();
    let (mut nodes, mut dangling) = (0usize, 0usize);
    let mut hist: BTreeMap<u8, usize> = BTreeMap::new();
    let mut sizes: BTreeMap<String, usize> = BTreeMap::new();
    for e in &entries {
        match full(&e.bytes) {
            Ok((t, ast)) => {
                nodes += ast.len();
                dangling += ast.dangling_refs();
                *sizes.entry("header+names".into()).or_default() += t.sections.first().map_or(0, |s| s.start);
                for s in &t.sections {
                    *sizes.entry(t.names.show(s.name)).or_default() += s.bytes.len();
                }
                for (_, n) in ast.nodes() {
                    *hist.entry(n.tag).or_default() += 1;
                }
            }
            Err(err) => failures.push(format!("{}: {err}", e.name)),
        }
    }
    let t_header = best(repeat, || {
        entries.iter().filter(|e| Tasty::parse(&e.bytes).is_ok()).count();
    });
    let t_full = best(repeat, || {
        entries.iter().filter(|e| full(&e.bytes).is_ok()).count();
    });

    let mb = total as f64 / 1e6;
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    println!("jar: {path}");
    println!("zip entries: {zip_entries}, tasty files: {} ({mb:.2} MB uncompressed)", entries.len());
    let parts: Vec<_> = sizes.iter().map(|(k, v)| format!("{k} {:.2}", *v as f64 / 1e6)).collect();
    println!("bytes by part (MB): {}", parts.join(", "));
    println!("parsed ok: {} failed: {}", entries.len() - failures.len(), failures.len());
    for f in failures.iter().take(20) {
        println!("  FAIL {f}");
    }
    println!("ast nodes: {nodes} dangling refs: {dangling}");
    println!("open jar + central dir (best of {repeat}): {:.2} ms", ms(t_dir));
    println!("open + inflate all .tasty (best of {repeat}): {:.2} ms", ms(t_unzip));
    println!("header+names+sections (best of {repeat}): {:.2} ms", ms(t_header));
    println!(
        "full parse incl. ASTs (best of {repeat}): {:.2} ms = {:.0} MB/s",
        ms(t_full),
        mb / t_full.as_secs_f64()
    );
    if args.contains(&"--tags".into()) {
        let mut v: Vec<_> = hist.into_iter().collect();
        v.sort_by_key(|e| std::cmp::Reverse(e.1));
        for (tag, n) in v {
            println!("  {:>9} {}", n, tags::name(tag));
        }
    }
    Ok(())
}

fn open(path: &str) -> Result<zip::ZipArchive<std::fs::File>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    zip::ZipArchive::new(file).map_err(|e| e.to_string())
}

fn load(path: &str) -> Result<Vec<Entry>, String> {
    let mut zip = open(path)?;
    let mut entries = Vec::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.name().ends_with(".tasty") {
            let mut bytes = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            entries.push(Entry { name: f.name().to_string(), bytes });
        }
    }
    Ok(entries)
}

fn full(bytes: &[u8]) -> tasty_reader::Result<(Tasty<'_>, Ast)> {
    let t = Tasty::parse(bytes)?;
    let ast = match t.section("ASTs") {
        Some(s) => Ast::parse(s.bytes)?,
        None => Ast::parse(&[])?,
    };
    Ok((t, ast))
}

fn best(n: u32, mut f: impl FnMut()) -> Duration {
    (0..n.max(1))
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .min()
        .unwrap_or_default()
}
