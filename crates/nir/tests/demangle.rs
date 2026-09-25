use nir::demangle::{sig, to_proxy};

#[test]
fn proxy_drops_result_and_scope() {
    assert_eq!(to_proxy("D3bariL16java.lang.StringEO").as_deref(), Some("P3bariE"));
    assert_eq!(to_proxy("D3bazL17java.lang.IntegeriEO").as_deref(), Some("P3bazL17java.lang.IntegerE"));
    assert_eq!(
        to_proxy("D19collectionClassNameL16java.lang.StringEO").as_deref(),
        Some("P19collectionClassNameE")
    );
    assert_eq!(to_proxy("F2idO"), None);
}

#[test]
fn renders_proxy_signatures() {
    assert_eq!(sig("P3bariE"), "proxy bar(int)");
}
