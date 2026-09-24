use std::io::Write;
use std::time::Instant;

use tree_sitter::{Node, Parser};

use crate::corpus::{Corpus, Source};

#[derive(Default)]
pub struct Stats {
    pub files: usize,
    pub failed: usize,
    pub errors: usize,
    pub missing: usize,
    pub nodes: usize,
}

pub fn parser() -> Parser {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_scala::LANGUAGE.into()).expect("scala grammar");
    parser
}

pub fn check(corpus: &Corpus, dump: &mut impl Write) -> Stats {
    let mut parser = parser();
    let mut stats = Stats::default();
    for src in &corpus.files {
        let tree = parser.parse(&src.text, None).expect("parse");
        let mut bad = Vec::new();
        stats.files += 1;
        stats.nodes += walk(tree.root_node(), &mut bad);
        if bad.is_empty() {
            continue;
        }
        stats.failed += 1;
        stats.errors += bad.iter().filter(|n| n.is_error()).count();
        stats.missing += bad.iter().filter(|n| n.is_missing()).count();
        report(src, &bad, dump);
    }
    stats
}

fn walk<'t>(node: Node<'t>, bad: &mut Vec<Node<'t>>) -> usize {
    if node.is_error() || node.is_missing() {
        bad.push(node);
    }
    if !node.has_error() {
        return node.descendant_count();
    }
    let mut cursor = node.walk();
    1 + node.children(&mut cursor).map(|c| walk(c, bad)).sum::<usize>()
}

fn report(src: &Source, bad: &[Node], dump: &mut impl Write) {
    let first = bad[0];
    let pos = first.start_position();
    let line = src.text.lines().nth(pos.row).unwrap_or("").trim();
    let kind = if first.is_missing() { format!("MISSING {}", first.kind()) } else { "ERROR".into() };
    let parent = first.parent().map_or("-", |p| p.kind());
    let _ = writeln!(
        dump,
        "{}\t{}:{}\t{}\t{}\t{}\t{}",
        src.path.display(),
        pos.row + 1,
        pos.column + 1,
        bad.len(),
        kind,
        parent,
        line.chars().take(160).collect::<String>()
    );
}

pub fn bench(corpus: &Corpus, iters: u32, visit: bool) -> f64 {
    let mut parser = parser();
    let start = Instant::now();
    for _ in 0..iters {
        for src in &corpus.files {
            let tree = parser.parse(&src.text, None).expect("parse");
            std::hint::black_box(if visit { visit_all(&tree) } else { 0 });
        }
    }
    start.elapsed().as_secs_f64()
}

fn visit_all(tree: &tree_sitter::Tree) -> usize {
    let mut cursor = tree.walk();
    let mut kinds = 0;
    loop {
        kinds += usize::from(cursor.node().kind_id());
        if cursor.goto_first_child() || cursor.goto_next_sibling() {
            continue;
        }
        loop {
            if !cursor.goto_parent() {
                return kinds;
            }
            if cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
