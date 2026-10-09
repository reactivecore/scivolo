//! The instance members of the Java classes the Scala library's implicit views convert to or its
//! classes extend, as the JDK (17 to 25) and the library's Java classes define them, inherited
//! ones included and `Object`'s aside.

/// The names of the instance members of Java class `class`, if known.
pub fn java_members(class: &str) -> Option<impl Iterator<Item = &'static str>> {
    let parts: &[&str] = match class {
        "scala.math.ScalaNumber" => &[NUMBER, "isWhole underlying"],
        "java.lang.Integer" | "java.lang.Long" => &[NUMBER, CONSTANT, "resolveConstantDesc"],
        "java.lang.Double" | "java.lang.Float" => &[NUMBER, CONSTANT, "resolveConstantDesc isNaN isInfinite"],
        "java.lang.Short" | "java.lang.Byte" => &[NUMBER, CONSTANT],
        "java.lang.Character" => &[CONSTANT, "charValue"],
        "java.lang.Boolean" => &[CONSTANT, "booleanValue"],
        "java.lang.Iterable" => &[ITERABLE],
        "java.util.Collection" | "java.util.Set" => &[ITERABLE, COLLECTION],
        "java.util.List" => &[ITERABLE, COLLECTION, LIST],
        "java.util.Map" | "java.util.concurrent.ConcurrentMap" => &[MAP],
        "java.util.Dictionary" => &["size isEmpty keys elements get put remove"],
        "java.util.Iterator" => &["hasNext next remove forEachRemaining"],
        "java.util.Enumeration" => &["hasMoreElements nextElement asIterator"],
        _ => return None,
    };
    Some(parts.iter().flat_map(|p| p.split_whitespace()))
}

const NUMBER: &str = "byteValue doubleValue floatValue intValue longValue shortValue";

const CONSTANT: &str = "compareTo describeConstable";

const ITERABLE: &str = "iterator forEach spliterator";

const COLLECTION: &str = "size isEmpty contains toArray add remove containsAll addAll removeAll removeIf \
    retainAll clear stream parallelStream";

const LIST: &str = "replaceAll sort get set indexOf lastIndexOf listIterator subList reversed addFirst \
    addLast getFirst getLast removeFirst removeLast";

const MAP: &str = "size isEmpty containsKey containsValue get put remove putAll clear keySet values \
    entrySet getOrDefault forEach replaceAll putIfAbsent replace computeIfAbsent computeIfPresent compute \
    merge";
