//! Main classes: the classes with a static `main(Array[String])`.

/// The mangled signature of the static `main(Array[String]): Unit` of a main class.
const MAIN: &str = "D4mainLAL16java.lang.String_uEo";

/// The class in NIR file `data` that defines `main`, if any.
pub fn defined(data: &[u8]) -> crate::Result<Option<String>> {
    let (mut pool, mut caches) = (crate::Pool::default(), crate::Caches::default());
    let index = crate::Reader::new(data, &mut caches, &mut pool)?.index()?;
    Ok(index.iter().find_map(|&(g, _)| match *pool.global(g) {
        crate::Global::Member(top, sig) if pool.str(sig) == MAIN => match *pool.global(top) {
            crate::Global::Top(name) => Some(pool.str(name).to_owned()),
            _ => None,
        },
        _ => None,
    }))
}
