#![allow(dead_code)]

mod ir;
mod program;
mod value;

use program::Program;
use std::time::Instant;

fn main() {
    let t0 = Instant::now();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (prog, t) = match Program::load(&args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };
    for (class, method) in prog.entries() {
        println!("entry: {class}.{method}");
    }
    eprintln!("read {:?}  parse {:?}  index {:?}  total {:?}", t.read, t.parse, t.index, t0.elapsed());
}
