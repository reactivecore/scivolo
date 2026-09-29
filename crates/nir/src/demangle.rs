//! Renders mangled member signatures (`Sig.mangle`, docs/contrib/mangling.md) as readable text.

pub fn sig(s: &str) -> String {
    let mut d = Demangler { chars: s.chars().collect(), pos: 0, out: String::new() };
    match d.sig() {
        Some(()) if d.pos == d.chars.len() => d.out,
        _ => s.to_string(),
    }
}

/// The reflective proxy signature `P<name><params>E` of method signature
/// `D<name><params><result>E<scope>`, as Scala Native's `Sig.toProxy` builds it.
pub fn to_proxy(s: &str) -> Option<String> {
    let mut d = Demangler { chars: s.chars().collect(), pos: 0, out: String::new() };
    if d.next()? != 'D' {
        return None;
    }
    d.ident()?;
    let mut result_start = None;
    while d.peek()? != 'E' {
        result_start = Some(d.pos);
        d.ty()?;
    }
    let name_and_params: String = d.chars[1..result_start?].iter().collect();
    Some(format!("P{name_and_params}E"))
}

/// The name and the mangled parameter and result types, result last, of public instance
/// method signature `D<name><params><result>EO`.
pub fn method(s: &str) -> Option<(String, Vec<String>)> {
    let mut d = Demangler { chars: s.chars().collect(), pos: 0, out: String::new() };
    if d.next()? != 'D' {
        return None;
    }
    let name = d.ident()?;
    let mut tys = Vec::new();
    while d.peek()? != 'E' {
        let start = d.pos;
        d.ty()?;
        tys.push(d.chars[start..d.pos].iter().collect());
    }
    (d.chars[d.pos + 1..] == ['O'] && !tys.is_empty()).then_some((name, tys))
}

pub fn extern_name(s: &str) -> Option<String> {
    let mut d = Demangler { chars: s.chars().collect(), pos: 0, out: String::new() };
    (d.next()? == 'C').then(|| d.ident())?
}

struct Demangler {
    chars: Vec<char>,
    pos: usize,
    out: String,
}

impl Demangler {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn number(&mut self) -> Option<usize> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect::<String>().parse().ok()
    }

    fn ident(&mut self) -> Option<String> {
        let mut units = self.number()?;
        if self.peek() == Some('-') {
            self.pos += 1;
        }
        let mut id = String::new();
        while units > 0 {
            let c = self.next()?;
            units = units.checked_sub(c.len_utf16())?;
            id.push(c);
        }
        Some(id)
    }

    fn sig(&mut self) -> Option<()> {
        match self.next()? {
            'F' => {
                let id = self.ident()?;
                self.scope()?;
                self.out.push_str(&id);
            }
            'R' => {
                self.out.push_str("<init>");
                self.params(false)?;
            }
            'I' => {
                self.next()?;
                self.out.push_str("<clinit>");
            }
            'D' | 'P' => {
                let proxy = self.chars[self.pos - 1] == 'P';
                let id = self.ident()?;
                self.out.push_str(if proxy { "proxy " } else { "" });
                self.out.push_str(&id);
                self.params(!proxy)?;
                if !proxy {
                    self.scope()?;
                }
            }
            'C' => {
                let id = self.ident()?;
                self.out.push_str(&format!("extern {id}"));
            }
            'G' => {
                let id = self.ident()?;
                self.out.push_str(&format!("generated {id}"));
            }
            'K' => {
                self.out.push_str("dup ");
                self.sig()?;
                self.params(false)?;
            }
            _ => return None,
        }
        Some(())
    }

    fn params(&mut self, with_result: bool) -> Option<()> {
        let mut tys = Vec::new();
        while self.peek()? != 'E' {
            tys.push(self.ty()?);
        }
        self.pos += 1;
        let ret = if with_result { tys.pop() } else { None };
        self.out.push_str(&format!("({})", tys.join(", ")));
        if let Some(r) = ret {
            self.out.push_str(&format!(": {r}"));
        }
        Some(())
    }

    fn scope(&mut self) -> Option<()> {
        match self.next()? {
            'O' => {}
            'o' => self.out.insert_str(0, "static "),
            c @ ('P' | 'p') => {
                let owner = self.global()?;
                let kw = if c == 'P' { "private" } else { "private static" };
                self.out.insert_str(0, &format!("{kw}[{owner}] "));
            }
            _ => return None,
        }
        Some(())
    }

    fn global(&mut self) -> Option<String> {
        match self.next()? {
            'T' => self.ident(),
            'M' => {
                let owner = self.ident()?;
                let saved = std::mem::take(&mut self.out);
                self.sig()?;
                let member = std::mem::replace(&mut self.out, saved);
                Some(format!("{owner}::{member}"))
            }
            _ => None,
        }
    }

    fn ty(&mut self) -> Option<String> {
        Some(match self.next()? {
            'v' => "...".into(),
            'z' => "bool".into(),
            'c' => "char".into(),
            'b' => "byte".into(),
            's' => "short".into(),
            'i' => "int".into(),
            'j' => "long".into(),
            'w' => "size".into(),
            'f' => "float".into(),
            'd' => "double".into(),
            'l' => "null".into(),
            'n' => "nothing".into(),
            'u' => "unit".into(),
            'R' if self.peek() == Some('_') => {
                self.pos += 1;
                "ptr".into()
            }
            'R' => {
                let mut tys = Vec::new();
                while self.peek()? != 'E' {
                    tys.push(self.ty()?);
                }
                self.pos += 1;
                let ret = tys.pop()?;
                format!("({}) => {ret}", tys.join(", "))
            }
            'A' => {
                let elem = self.ty()?;
                let n =
                    if self.peek()?.is_ascii_digit() { self.number()?.to_string() } else { String::new() };
                self.next()?;
                if n.is_empty() { format!("array[{elem}]") } else { format!("[{elem} x {n}]") }
            }
            'S' => {
                let mut tys = Vec::new();
                while self.peek()? != 'E' {
                    tys.push(self.ty()?);
                }
                self.pos += 1;
                format!("{{{}}}", tys.join(", "))
            }
            'L' | 'X' => self.ty()?,
            c if c.is_ascii_digit() => {
                self.pos -= 1;
                self.ident()?
            }
            _ => return None,
        })
    }
}
