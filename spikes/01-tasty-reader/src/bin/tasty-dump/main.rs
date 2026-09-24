mod jar;

use std::process::ExitCode;
use tasty_reader::{Ast, Attribute, Tasty, attribute_name, dump};

const USAGE: &str = "usage: tasty-dump [--names] <file.tasty>
       tasty-dump <file.jar> [--repeat N] [--tags] [--dump ENTRY-SUBSTRING]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.iter().find(|a| !a.starts_with("--") && !is_flag_value(&args, a)) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let result = if path.ends_with(".jar") {
        jar::run(path, &args)
    } else {
        std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|b| dump_file(&b, args.contains(&"--names".into())))
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn is_flag_value(args: &[String], a: &String) -> bool {
    args.windows(2).any(|w| std::ptr::eq(&w[1], a) && matches!(w[0].as_str(), "--repeat" | "--dump"))
}

pub fn dump_file(bytes: &[u8], with_names: bool) -> Result<(), String> {
    let t = Tasty::parse(bytes).map_err(|e| e.to_string())?;
    let h = &t.header;
    println!(
        "TASTy {}.{}-{} tooling={:?} uuid={}",
        h.major,
        h.minor,
        h.experimental,
        h.tooling,
        h.uuid_string()
    );
    println!("names: {}", t.names.len());
    if with_names {
        for (r, n) in t.names.iter() {
            println!("  {:>4}: {:<60} {:?}", r.0, t.names.show_signed(r), n);
        }
    }
    for s in &t.sections {
        println!("section {} @{} ({} bytes)", t.names.show(s.name), s.start, s.bytes.len());
    }
    for a in t.attributes().map_err(|e| e.to_string())? {
        match a {
            Attribute::Flag(f) => println!("attribute {}", attribute_name(f)),
            Attribute::SourceFile(r) => println!("attribute SOURCEFILE {}", t.names.show(r)),
        }
    }
    let sec = t.section("ASTs").ok_or("no ASTs section")?;
    let ast = Ast::parse(sec.bytes).map_err(|e| e.to_string())?;
    println!("ast nodes: {} dangling refs: {}\n", ast.len(), ast.dangling_refs());
    print!("{}", dump(&ast, &t.names));
    Ok(())
}
