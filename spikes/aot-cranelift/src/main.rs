//! NIR → Cranelift spike: translates NIR definitions to an object file and measures codegen and link
//! times. `aot-cranelift [--opt none|speed] [--eh flag|none] [--verify] [--link] [--run N] OUT.o INPUT...`
//! where INPUT is a directory of `.nir` files or a jar.

mod translate;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

struct Args {
    opt: String,
    eh_flag: bool,
    verify: bool,
    disasm: bool,
    debug: bool,
    link: bool,
    run: Option<u32>,
    out: PathBuf,
    inputs: Vec<PathBuf>,
}

fn parse_args() -> Args {
    let mut a = Args {
        opt: "speed".into(),
        eh_flag: true,
        verify: false,
        disasm: false,
        debug: false,
        link: false,
        run: None,
        out: PathBuf::new(),
        inputs: Vec::new(),
    };
    let mut it = std::env::args().skip(1);
    let mut positional = Vec::new();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--opt" => a.opt = it.next().expect("--opt LEVEL"),
            "--eh" => a.eh_flag = it.next().expect("--eh flag|none") == "flag",
            "--verify" => a.verify = true,
            "--disasm" => a.disasm = true,
            "--debug" => a.debug = true,
            "--link" => a.link = true,
            "--run" => a.run = Some(it.next().expect("--run N").parse().expect("number")),
            _ => positional.push(PathBuf::from(arg)),
        }
    }
    assert!(positional.len() >= 2, "usage: aot-cranelift [flags] OUT.o INPUT...");
    a.out = positional.remove(0);
    a.inputs = positional;
    a
}

fn nir_files(input: &Path) -> Vec<(String, Vec<u8>)> {
    if input.is_dir() {
        let mut out = Vec::new();
        for e in std::fs::read_dir(input).expect("read dir") {
            let p = e.expect("entry").path();
            if p.extension().is_some_and(|x| x == "nir") {
                out.push((p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap()));
            }
        }
        out.sort();
        return out;
    }
    let file = std::fs::File::open(input).expect("open jar");
    let mut zip = zip::ZipArchive::new(file).expect("zip");
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).expect("entry");
        if entry.name().ends_with(".nir") {
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            std::io::copy(&mut entry, &mut bytes).expect("inflate");
            out.push((entry.name().to_string(), bytes));
        }
    }
    out
}

fn main() {
    let args = parse_args();
    let t0 = Instant::now();
    let mut files = Vec::new();
    for input in &args.inputs {
        files.extend(nir_files(input));
    }
    let bytes: usize = files.iter().map(|(_, b)| b.len()).sum();
    let t_read = t0.elapsed();

    let t1 = Instant::now();
    let mut pool = nir::Pool::default();
    let mut caches = nir::Caches::default();
    let mut defns = Vec::new();
    for (name, data) in &files {
        let mut r = nir::Reader::new(data, &mut caches, &mut pool).unwrap_or_else(|e| panic!("{name}: {e}"));
        r.debug_info = false;
        defns.extend(r.all().unwrap_or_else(|e| panic!("{name}: {e}")));
    }
    let t_decode = t1.elapsed();
    eprintln!(
        "read {} files, {:.1} MB in {:.1} ms; decoded {} definitions in {:.1} ms",
        files.len(),
        bytes as f64 / 1e6,
        t_read.as_secs_f64() * 1e3,
        defns.len(),
        t_decode.as_secs_f64() * 1e3
    );

    std::panic::set_hook(Box::new(|_| {}));
    let t2 = Instant::now();
    let mut tr = translate::Translator::new(&pool, &args.opt, args.eh_flag, args.verify);
    tr.disasm = args.disasm;
    tr.declare_all(&defns);
    let t_declare = t2.elapsed();
    let t3 = Instant::now();
    let stats = tr.define_all(&defns);
    let t_define = t3.elapsed();
    let t4 = Instant::now();
    if args.link {
        tr.selftest();
    }
    let object = tr.finish();
    let t_emit = t4.elapsed();
    if let Some(dir) = args.out.parent() {
        std::fs::create_dir_all(dir).expect("create output directory");
    }
    std::fs::write(&args.out, &object).expect("write object");
    eprintln!(
        "declare {:.1} ms; translate+compile {:.1} ms ({} functions, {} bailed: {:?}, {} NIR insts, \
         {:.2} µs/inst); emit {:.1} ms; object {:.1} MB",
        t_declare.as_secs_f64() * 1e3,
        t_define.as_secs_f64() * 1e3,
        stats.functions,
        stats.bailed,
        stats.bail_reasons,
        stats.insts,
        t_define.as_secs_f64() * 1e6 / stats.insts.max(1) as f64,
        t_emit.as_secs_f64() * 1e3,
        object.len() as f64 / 1e6
    );

    if args.link {
        link(&args);
    }
}

fn run(cmd: &mut Command, what: &str) -> f64 {
    let t = Instant::now();
    let status = cmd.status().unwrap_or_else(|e| panic!("{what}: {e}"));
    assert!(status.success(), "{what} failed: {status}");
    t.elapsed().as_secs_f64() * 1e3
}

fn link(args: &Args) {
    let dir = args.out.parent().unwrap_or(Path::new("."));
    let c = Path::new(env!("CARGO_MANIFEST_DIR")).join("c");
    let rt_o = dir.join("rt.o");
    let main_o = dir.join("main.o");
    let bin = dir.join("fib-bin");
    run(Command::new("cc").args(["-O1", "-c", "-o"]).arg(&rt_o).arg(c.join("rt.c")), "cc rt.c");
    run(Command::new("cc").args(["-O1", "-c", "-o"]).arg(&main_o).arg(c.join("main.c")), "cc main.c");
    let mut times = Vec::new();
    for dead_strip in [false, true] {
        for _ in 0..3 {
            let mut cmd = Command::new("cc");
            cmd.arg("-o").arg(&bin).arg(&args.out).arg(&rt_o).arg(&main_o);
            cmd.args(["-Wl,-undefined,dynamic_lookup"]);
            if dead_strip {
                cmd.arg("-Wl,-dead_strip");
            }
            times.push((dead_strip, run(&mut cmd, "link")));
        }
    }
    for (dead_strip, ms) in &times {
        eprintln!("link (dead_strip={dead_strip}): {ms:.1} ms");
    }
    let size = std::fs::metadata(&bin).map(|m| m.len()).unwrap_or(0);
    eprintln!("binary {} bytes", size);
    if args.debug {
        let out = Command::new("lldb")
            .args(["--batch", "-o", "run", "-o", "bt", "-o", "disassemble --pc", "-o", "register read", "--"])
            .arg(&bin)
            .arg("1")
            .output()
            .expect("lldb");
        eprintln!("{}\n{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    }
    if let Some(n) = args.run {
        for _ in 0..3 {
            let t = Instant::now();
            let out = Command::new(&bin).arg(n.to_string()).output().expect("run");
            let ms = t.elapsed().as_secs_f64() * 1e3;
            eprintln!(
                "run {n}: {:.1} ms, {} ({}) stderr: {}",
                ms,
                String::from_utf8_lossy(&out.stdout).trim(),
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
    }
}
