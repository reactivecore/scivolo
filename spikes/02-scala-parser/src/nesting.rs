//! Flags definitions whose tree-sitter nesting disagrees with the source indentation.

use std::collections::HashMap;
use std::io::Write;

use tree_sitter::Node;

use crate::corpus::Corpus;
use crate::ts;

const DEFINITIONS: &[&str] = &[
    "class_definition",
    "object_definition",
    "trait_definition",
    "enum_definition",
    "function_definition",
    "function_declaration",
    "val_definition",
    "var_definition",
    "val_declaration",
    "given_definition",
    "extension_definition",
    "type_definition",
    "package_object",
];

struct File<'t> {
    indents: Vec<Option<usize>>,
    lines: Vec<&'t str>,
    starts: HashMap<usize, usize>,
    rows: HashMap<usize, (usize, usize)>,
    defs: Vec<(Node<'t>, Vec<usize>)>,
}

fn indents(text: &str) -> Vec<Option<usize>> {
    let indent = |line: &str| {
        let body = line.trim_start();
        let skip =
            body.is_empty() || body.starts_with("//") || body.starts_with('*') || body.starts_with("/*");
        (!skip).then(|| line.len() - body.len())
    };
    text.lines().map(indent).collect()
}

fn collect<'t>(node: Node<'t>, file: &mut File<'t>, stack: &mut Vec<usize>) {
    let is_def = DEFINITIONS.contains(&node.kind());
    if is_def {
        let row = node.start_position().row;
        file.rows.insert(node.id(), (row, node.start_byte()));
        if file.indents.get(row).copied().flatten() == Some(node.start_position().column) {
            file.starts.entry(row).or_insert(node.id());
            file.defs.push((node, stack.clone()));
        }
        stack.push(node.id());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect(child, file, stack);
    }
    if is_def {
        stack.pop();
    }
}

fn opens_block(line: &str) -> bool {
    let code = line.split("//").next().unwrap_or("").trim_end();
    [":", "=", "=>", "{", "with", "then", "else", "do"].iter().any(|o| code.ends_with(o))
}

fn suspect(file: &File, src: &str, node: Node, ancestors: &[usize]) -> Option<&'static str> {
    let row = node.start_position().row;
    let indent = file.indents[row]?;
    if let Some(&(parent_row, parent_byte)) = ancestors.last().and_then(|id| file.rows.get(id))
        && parent_row != row
        && file.indents[parent_row].is_some_and(|p| p >= indent)
        && !src[parent_byte..node.start_byte()].contains('{')
    {
        return Some("swallowed");
    }
    let above = (0..row).rev().find(|&r| file.indents[r].is_some_and(|i| i < indent))?;
    let owner = file.starts.get(&above)?;
    let opens = file.lines.get(above).is_some_and(|l| opens_block(l));
    (opens && !ancestors.contains(owner)).then_some("escaped")
}

pub fn check(corpus: &Corpus, dump: &mut impl Write) -> (usize, usize) {
    let mut parser = ts::parser();
    let (mut files, mut defs) = (0, 0);
    for src in &corpus.files {
        let tree = parser.parse(&src.text, None).expect("parse");
        let mut file = File {
            indents: indents(&src.text),
            lines: src.text.lines().collect(),
            starts: HashMap::new(),
            rows: HashMap::new(),
            defs: Vec::new(),
        };
        collect(tree.root_node(), &mut file, &mut Vec::new());
        let bad: Vec<_> = file
            .defs
            .iter()
            .filter_map(|(n, anc)| suspect(&file, &src.text, *n, anc).map(|why| (n, why)))
            .collect();
        if !bad.is_empty() {
            files += 1;
            defs += bad.len();
        }
        for (node, why) in bad.iter().take(3) {
            let row = node.start_position().row;
            let line = src.text.lines().nth(row).unwrap_or("").trim();
            let chain: Vec<String> = std::iter::successors(node.parent(), |n| n.parent())
                .filter(|n| DEFINITIONS.contains(&n.kind()))
                .map(|n| format!("{}@{}", n.kind(), n.start_position().row + 1))
                .collect();
            let chain = chain.join("<");
            writeln!(dump, "{}\t{}\t{why}\t{}\t{chain}\t{line}", src.path.display(), row + 1, bad.len()).ok();
        }
    }
    (files, defs)
}
