mod source;
mod stats;

use clap::Parser;
use nir::{Caches, Pool, Reader, cursor, show};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// Prints, parses, indexes or counts NIR files.
#[derive(Parser)]
#[command(name = "nir-dump")]
struct Args {
    #[command(flatten)]
    mode: ModeFlags,
    /// Print names mangled.
    #[arg(long)]
    mangled: bool,
    /// List the extern definitions in the statistics.
    #[arg(long)]
    externs: bool,
    /// Dump definitions with their positions, local names and scopes.
    #[arg(long)]
    debug: bool,
    /// Dump the definitions of each file sorted by their text.
    #[arg(long)]
    sorted: bool,
    /// Dump only definitions whose name contains TEXT.
    #[arg(long, value_name = "TEXT")]
    filter: Option<String>,
    /// Time the best of N runs.
    #[arg(long, value_name = "N", default_value_t = 1)]
    repeat: usize,
    /// A .nir file, a directory or a jar.
    path: PathBuf,
}

#[derive(clap::Args)]
#[group(multiple = false)]
struct ModeFlags {
    /// Print the definitions as text (the default, except for a jar).
    #[arg(long)]
    dump: bool,
    /// Parse all definitions (the default for a jar).
    #[arg(long)]
    parse: bool,
    /// Index the definitions.
    #[arg(long)]
    index: bool,
    /// Parse and print statistics.
    #[arg(long)]
    stats: bool,
}

enum Mode {
    Dump,
    Parse,
    Index,
    Stats,
}

fn main() -> ExitCode {
    let args = Args::parse();
    let t = Instant::now();
    let entries = match source::load(&args.path) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{}: {e}", args.path.display());
            return ExitCode::FAILURE;
        }
    };
    let load_time = t.elapsed();
    let is_jar = args.path.extension().is_some_and(|e| e == "jar");
    let mode = match args.mode {
        ModeFlags { parse: true, .. } => Mode::Parse,
        ModeFlags { index: true, .. } => Mode::Index,
        ModeFlags { stats: true, .. } => Mode::Stats,
        ModeFlags { dump: false, .. } if is_jar => Mode::Parse,
        _ => Mode::Dump,
    };
    let bytes: usize = entries.iter().map(|e| e.data.len()).sum();
    println!("# {} .nir files, {} bytes, loaded in {}", entries.len(), bytes, ms(load_time));
    let failures = match mode {
        Mode::Dump => dump(&entries, &args),
        Mode::Index => timed(&args, "index", || index(&entries)),
        Mode::Parse => timed(&args, "parse", || parse(&entries, None)),
        Mode::Stats => {
            let mut st = stats::Stats::default();
            let (failures, summary) = parse(&entries, Some(&mut st));
            println!("# {summary}");
            st.print(args.externs);
            failures
        }
    };
    if failures > 0 { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

fn ms(d: Duration) -> String {
    format!("{:.2} ms", d.as_secs_f64() * 1e3)
}

fn timed(args: &Args, what: &str, mut run: impl FnMut() -> (usize, String)) -> usize {
    let mut best = Duration::MAX;
    let mut result = (0, String::new());
    for _ in 0..args.repeat.max(1) {
        let t = Instant::now();
        result = run();
        best = best.min(t.elapsed());
    }
    println!("# {what}: {} (best of {}); {}", ms(best), args.repeat.max(1), result.1);
    result.0
}

fn report_err(name: &str, e: &cursor::Error, failures: &mut usize) {
    *failures += 1;
    if *failures <= 10 {
        eprintln!("{name}: {e}");
    }
}

fn index(entries: &[source::Entry]) -> (usize, String) {
    let (mut pool, mut caches) = (Pool::default(), Caches::default());
    let (mut failures, mut defns) = (0, 0);
    for e in entries {
        match Reader::new(&e.data, &mut caches, &mut pool).and_then(|mut r| r.index()) {
            Ok(idx) => defns += idx.len(),
            Err(err) => report_err(&e.name, &err, &mut failures),
        }
    }
    let ok = entries.len() - failures;
    (
        failures,
        format!(
            "{ok}/{} files ok, {defns} definitions indexed, {} strings",
            entries.len(),
            pool.strings.len()
        ),
    )
}

fn parse(entries: &[source::Entry], mut st: Option<&mut stats::Stats>) -> (usize, String) {
    let (mut pool, mut caches) = (Pool::default(), Caches::default());
    let (mut failures, mut defns) = (0, 0);
    for e in entries {
        let result = Reader::new(&e.data, &mut caches, &mut pool).and_then(|mut r| r.all());
        match result {
            Ok(ds) => {
                defns += ds.len();
                if let Some(st) = st.as_deref_mut() {
                    st.add_file(&pool, &ds);
                }
            }
            Err(err) => report_err(&e.name, &err, &mut failures),
        }
    }
    let ok = entries.len() - failures;
    let summary = format!(
        "{ok}/{} files ok, {defns} definitions; pool: {} strings, {} globals, {} types, {} vals",
        entries.len(),
        pool.strings.len(),
        pool.globals.len(),
        pool.types.len(),
        pool.vals.len()
    );
    (failures, summary)
}

fn dump(entries: &[source::Entry], args: &Args) -> usize {
    let (mut pool, mut caches) = (Pool::default(), Caches::default());
    let mut failures = 0;
    for e in entries {
        let result = Reader::new(&e.data, &mut caches, &mut pool).and_then(|mut r| {
            let idx = r.index()?;
            let wanted: Vec<u32> = idx
                .iter()
                .filter(|(g, _)| {
                    let name = show::global(r.pool, *g, true);
                    args.filter.as_ref().is_none_or(|f| name.contains(f.as_str()))
                })
                .map(|&(_, off)| off)
                .collect();
            wanted.into_iter().map(|off| r.defn_at(off)).collect::<cursor::Result<Vec<_>>>()
        });
        match result {
            Ok(defns) if !defns.is_empty() => {
                println!("\n// {}", e.name);
                let mut texts: Vec<String> = defns
                    .iter()
                    .map(|d| match args.debug {
                        true => show::defn_with_debug_info(&pool, d),
                        false => show::defn(&pool, d, !args.mangled),
                    })
                    .collect();
                if args.sorted {
                    texts.sort();
                }
                for t in texts {
                    println!("{t}\n");
                }
            }
            Ok(_) => {}
            Err(err) => report_err(&e.name, &err, &mut failures),
        }
    }
    failures
}
