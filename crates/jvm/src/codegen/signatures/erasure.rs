//! Whether a generic signature erases to a descriptor, type variables matching any type.

/// Whether method or field signature `sig` erases to descriptor `desc`.
pub fn erases_to(sig: &str, desc: &str) -> bool {
    let (sig, desc) = (types(sig), types(desc));
    sig.len() == desc.len() && sig.iter().zip(&desc).all(|(s, d)| s == d || s.ends_with('?'))
}

/// The erased types of a signature in order, `?` for a type variable, past its type parameters.
fn types(sig: &str) -> Vec<String> {
    let b = sig.as_bytes();
    let mut i = 0;
    if b.first() == Some(&b'<') {
        let mut depth = 0;
        while i < b.len() {
            depth += match b[i] {
                b'<' => 1,
                b'>' => -1,
                _ => 0,
            };
            i += 1;
            if depth == 0 {
                break;
            }
        }
    }
    let mut out = Vec::new();
    let mut prefix = String::new();
    while i < b.len() {
        match b[i] {
            b'(' | b')' => i += 1,
            b'^' => break,
            b'[' => {
                prefix.push('[');
                i += 1;
            }
            b'T' => {
                while i < b.len() && b[i] != b';' {
                    i += 1;
                }
                i += 1;
                out.push(format!("{}?", std::mem::take(&mut prefix)));
            }
            b'L' => {
                let mut name = String::from("L");
                let mut depth = 0;
                i += 1;
                while i < b.len() && !(depth == 0 && b[i] == b';') {
                    match b[i] {
                        b'<' => depth += 1,
                        b'>' => depth -= 1,
                        b'.' if depth == 0 => name.push('$'),
                        c if depth == 0 => name.push(c as char),
                        _ => {}
                    }
                    i += 1;
                }
                i += 1;
                out.push(format!("{}{name};", std::mem::take(&mut prefix)));
            }
            c => {
                i += 1;
                out.push(format!("{}{}", std::mem::take(&mut prefix), c as char));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::erases_to;

    #[test]
    fn erases_signatures() {
        assert!(erases_to("<T:Ljava/lang/Object;>([TT;)I", "(Ljava/lang/Object;)I"));
        assert!(erases_to(
            "()Lscala/Function1<Ljava/lang/Object;Ljava/lang/Object;>;",
            "()Lscala/Function1;"
        ));
        assert!(erases_to("()LBox<TA;>.Nested$;", "()LBox$Nested$;"));
        assert!(!erases_to(
            "(Lscala/Product;)LSingle$;",
            "(Lscala/Product;)Lscala/deriving/Mirror$Singleton;"
        ));
    }
}
