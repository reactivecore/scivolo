// Derived from Scala Native 0.5.12 (Apache-2.0, see native/LICENSE-scala-native.md); translated to Rust and changed:
//   nir/src/main/scala/scala/scalanative/nir/Attrs.scala

use nir::Attr;

/// `attrs` as Scala Native's `Attrs.fromSeq(attrs).toSeq`: the last of each kind of inlining,
/// specialization and optimization, each flag once, links and definitions last.
pub fn canonical(attrs: Vec<Attr>) -> Vec<Attr> {
    let mut out: Vec<Attr> = Vec::new();
    for a in attrs {
        let r = rank(&a);
        let repeatable = matches!(a, Attr::Link(_) | Attr::Define(_));
        match out.iter().position(|o| !repeatable && rank(o) == r) {
            Some(i) => out[i] = a,
            None => out.push(a),
        }
    }
    out.retain(|a| !matches!(a, Attr::MayInline | Attr::MaySpecialize | Attr::UnOpt));
    out.sort_by_key(rank);
    out
}

fn rank(a: &Attr) -> u8 {
    match a {
        Attr::MayInline | Attr::InlineHint | Attr::NoInline | Attr::AlwaysInline => 0,
        Attr::MaySpecialize | Attr::NoSpecialize => 1,
        Attr::UnOpt | Attr::NoOpt | Attr::DidOpt | Attr::BailOpt(_) => 2,
        Attr::Align { .. } => 3,
        Attr::Extern { .. } => 4,
        Attr::Dyn => 5,
        Attr::Stub => 6,
        Attr::LinkCppRuntime => 7,
        Attr::Abstract => 8,
        Attr::Volatile => 9,
        Attr::Final => 10,
        Attr::SafePublish => 11,
        Attr::LinktimeResolved => 12,
        Attr::UsesIntrinsic => 13,
        Attr::Link(_) => 14,
        Attr::Define(_) => 15,
    }
}
