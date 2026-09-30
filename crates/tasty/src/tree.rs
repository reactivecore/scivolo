use crate::cursor::{Cursor, Result};
use crate::tags::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

/// One tagged tree. `a` holds the first Nat/Int payload (name ref, address, constant bits), `b` the second.
#[derive(Debug, Clone)]
pub struct Node {
    pub addr: u32,
    pub tag: u8,
    pub a: i64,
    pub b: i64,
    kids_start: u32,
    kids_len: u32,
}

/// Structural decode of an ASTs section; nodes are stored in preorder, so addresses are ascending.
pub struct Ast {
    nodes: Vec<Node>,
    kids: Vec<NodeId>,
    roots: Vec<NodeId>,
}

const MAX_DEPTH: u32 = 4096;

impl Ast {
    pub fn parse(section: &[u8]) -> Result<Ast> {
        let mut p = Parser {
            cur: Cursor::new(section),
            ast: Ast { nodes: Vec::with_capacity(section.len() / 3), kids: Vec::new(), roots: Vec::new() },
            stack: Vec::new(),
        };
        while !p.cur.at_end() {
            let id = p.tree(0)?;
            p.ast.roots.push(id);
        }
        Ok(p.ast)
    }

    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0 as usize]
    }

    pub fn tag(&self, id: NodeId) -> u8 {
        self.node(id).tag
    }

    pub fn kids(&self, id: NodeId) -> &[NodeId] {
        let n = self.node(id);
        &self.kids[n.kids_start as usize..(n.kids_start + n.kids_len) as usize]
    }

    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.nodes.iter().enumerate().map(|(i, n)| (NodeId(i as u32), n))
    }

    /// The node starting at a given AST address (target of SHARED*, *REFdirect, *REFsymbol, ...).
    pub fn at(&self, addr: u32) -> Option<NodeId> {
        let i = self.nodes.binary_search_by_key(&addr, |n| n.addr).ok()?;
        Some(NodeId(i as u32))
    }

    /// Follows SHAREDterm/SHAREDtype links.
    pub fn deref(&self, mut id: NodeId) -> NodeId {
        for _ in 0..64 {
            let n = self.node(id);
            if !matches!(n.tag, SHAREDterm | SHAREDtype) {
                break;
            }
            let Some(t) = self.at(n.a as u32) else { break };
            id = t;
        }
        id
    }

    /// Address payloads that do not land on a node start.
    pub fn dangling_refs(&self) -> usize {
        self.nodes.iter().filter(|n| ref_addr(n).is_some_and(|a| self.at(a).is_none())).count()
    }
}

pub fn ref_addr(n: &Node) -> Option<u32> {
    match n.tag {
        SHAREDterm | SHAREDtype | TERMREFdirect | TYPEREFdirect | RECthis | TERMREFsymbol | TYPEREFsymbol
        | RETURN | PARAMtype => Some(n.a as u32),
        _ => None,
    }
}

struct Parser<'a> {
    cur: Cursor<'a>,
    ast: Ast,
    stack: Vec<NodeId>,
}

impl Parser<'_> {
    fn push(&mut self, addr: usize, tag: u8, a: i64) -> NodeId {
        let id = NodeId(self.ast.nodes.len() as u32);
        let n = Node { addr: addr as u32, tag, a, b: 0, kids_start: 0, kids_len: 0 };
        self.ast.nodes.push(n);
        id
    }

    fn tree(&mut self, depth: u32) -> Result<NodeId> {
        if depth > MAX_DEPTH {
            return self.cur.err("tree too deep");
        }
        let addr = self.cur.pos();
        let tag = self.cur.byte()?;
        let base = self.stack.len();
        let id = match tag {
            0 | 1 => return self.cur.err(format!("invalid tag {tag}")),
            _ if tag < FIRST_NAT => self.push(addr, tag, 0),
            _ if tag < FIRST_AST => {
                let a = if is_signed_payload(tag) { self.cur.long_int()? } else { self.cur.nat()? as i64 };
                if matches!(tag, SHAREDterm | SHAREDtype) && a as usize >= addr {
                    return self.cur.err(format!("shared tree at {addr} refers forward to {a}"));
                }
                self.push(addr, tag, a)
            }
            _ if tag < FIRST_NAT_AST => {
                let id = self.push(addr, tag, 0);
                self.kid(depth)?;
                id
            }
            _ if tag < FIRST_LENGTH => {
                let a = self.cur.nat()? as i64;
                let id = self.push(addr, tag, a);
                self.kid(depth)?;
                id
            }
            _ => return self.length_tree(addr, tag, depth),
        };
        self.close(id, base);
        Ok(id)
    }

    fn kid(&mut self, depth: u32) -> Result<()> {
        let k = self.tree(depth + 1)?;
        self.stack.push(k);
        Ok(())
    }

    fn close(&mut self, id: NodeId, base: usize) {
        let start = self.ast.kids.len() as u32;
        self.ast.kids.extend(self.stack.drain(base..));
        let n = &mut self.ast.nodes[id.0 as usize];
        n.kids_start = start;
        n.kids_len = self.ast.kids.len() as u32 - start;
    }

    fn length_tree(&mut self, addr: usize, tag: u8, depth: u32) -> Result<NodeId> {
        let base = self.stack.len();
        let end = self.cur.end()?;
        let id = self.push(addr, tag, 0);
        let nats = leading_nats(tag);
        if nats > 0 {
            self.ast.nodes[id.0 as usize].a = self.cur.nat()? as i64;
        }
        if nats > 1 {
            self.ast.nodes[id.0 as usize].b = self.cur.nat()? as i64;
        }
        if is_methodic(tag) {
            self.kid(depth)?;
            while self.cur.pos() < end {
                let modifier = (PRIVATE..FIRST_NAT).contains(&self.cur.peek()?);
                self.kid(depth)?;
                if !modifier {
                    let at = self.cur.pos();
                    let name = self.cur.nat()? as i64;
                    let k = self.push(at, NAMEref, name);
                    self.stack.push(k);
                }
            }
        } else {
            while self.cur.pos() < end {
                self.kid(depth)?;
            }
        }
        if self.cur.pos() != end {
            return self.cur.err(format!("{} overran its length (end {end})", name(tag)));
        }
        self.close(id, base);
        Ok(id)
    }
}
