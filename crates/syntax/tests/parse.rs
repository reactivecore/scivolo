//! The parser on small inputs, against trees recorded from dotty 3.9.0; `just syntax-report`
//! compares it with dotty on its test suites.

use syntax::ast::print::Printer;
use syntax::config::Config;
use syntax::names::Names;
use syntax::parse::Parser;
use syntax::source::SourceFile;

/// The tree of `text` and its diagnostics, as `syntax-dump trees` prints them.
fn parse(text: &str) -> String {
    let source = SourceFile::new("t.scala", text);
    let mut names = Names::default();
    let mut p = Parser::new(&source, &mut names, Config::default());
    let root = p.parse();
    let mut printer = Printer { ast: &p.ast, names: p.s.names, out: String::new() };
    printer.tree(root);
    let mut out = printer.out;
    for d in p.s.diags.unique() {
        out.push_str(&format!("\n{} {}", d.pos(), d.message));
    }
    out
}

#[test]
fn infix_operators_follow_precedence_and_associativity() {
    assert_eq!(
        parse("object A { val x = 1 + 2 * 3 :: Nil }\n"),
        "(PackageDef 0..37 (Ident 0..0 t:<empty>) [ (ModuleDef 0..37@7 (Mods [module] [] []) t:A (Template 11..35 \
         (DefDef 11..11 (Mods [] [] []) t:<init> [] (TypeTree 11..11) _) [] _self [ (ValDef 11..35@15 (Mods [] [] []) \
         t:x (TypeTree 16..16) (InfixOp 19..35@29 (InfixOp 19..28@21 (Number 19..20@19 \"1\" Whole(10)) (Ident \
         21..22@21 t:+) (InfixOp 23..28@25 (Number 23..24@23 \"2\" Whole(10)) (Ident 25..26@25 t:*) (Number 27..28@27 \
         \"3\" Whole(10)))) (Ident 29..31@29 t:::) (Ident 32..35@32 t:Nil)))]))])"
    );
}

#[test]
fn indented_class_with_using_clause_and_end_marker() {
    let text =
        "class C[T](val t: T) extends B:\n  def f(using Ordering[T]): Int =\n    val y = 1\n    y\n  end f\n";
    assert_eq!(
        parse(text),
        "(PackageDef 0..93 (Ident 0..0 t:<empty>) [ (TypeDef 0..93@6 (Mods [] [] []) T:C (Template 7..93 (DefDef \
         7..20@7 (Mods [] [] []) t:<init> [ [ (TypeDef 8..9@8 (Mods [private[this] <param>] [] []) T:T (TypeBoundsTree \
         8..8 _ _ _))] [ (ValDef 11..19@15 (Mods [<paramaccessor>] [] []) t:t (Ident 18..19@18 T:T) _)]] (TypeTree \
         19..19) _) [ (Ident 29..30@29 T:B)] _self [ (DefDef 34..93@38 (Mods [<method>] [] []) t:f [ [ (ValDef 46..57 \
         (Mods [<param> <synthetic> given] [] []) t:x$1 (AppliedTypeTree 46..57@46 (Ident 46..54@46 T:Ordering) [ \
         (Ident 55..56@55 T:T)]) _)]] (Ident 60..63@60 T:Int) (Block 70..85@70 [ (ValDef 70..79@74 (Mods [] [] []) \
         t:y (TypeTree 75..75) (Number 78..79@78 \"1\" Whole(10)))] (Ident 84..85@84 t:y)))]))])"
    );
}

#[test]
fn givens_and_extensions() {
    let text = "given intOrd: Ordering[Int]:\n  def compare(a: Int, b: Int) = 0\n\
                given [T: Ordering] => Ordering[List[T]] = ???\n\
                extension (s: String) def twice = s + s\n";
    assert_eq!(
        parse(text),
        "(PackageDef 0..149 (Ident 0..0 t:<empty>) [ (ModuleDef 0..62@6 (Mods [given] [] [ (Given 0..5@0)]) \
         t:intOrd (Template 14..62 (DefDef 14..14 (Mods [] [] []) t:<init> [] (TypeTree 14..14) _) [ \
         (AppliedTypeTree 14..27@14 (Ident 14..22@14 T:Ordering) [ (Ident 23..26@23 T:Int)])] _self [ (DefDef \
         31..62@35 (Mods [<method>] [] []) t:compare [ [ (ValDef 43..49@43 (Mods [<param>] [] []) t:a (Ident \
         46..49@46 T:Int) _) (ValDef 51..57@51 (Mods [<param>] [] []) t:b (Ident 54..57@54 T:Int) _)]] (TypeTree \
         58..58) (Number 61..62@61 \"0\" Whole(10)))])) (DefDef 63..109@69 (Mods [final given] [] [ (Given \
         63..68@63)]) t: [ [ (TypeDef 70..81@70 (Mods [<param>] [] []) T:T (ContextBounds 73..81@73 (TypeBoundsTree \
         73..73 _ _ _) [ (ContextBoundTypeTree 73..81@81 (Ident 73..81@73 T:Ordering) T:T t:)]))]] (AppliedTypeTree \
         86..103@86 (Ident 86..94@86 T:Ordering) [ (AppliedTypeTree 95..102@95 (Ident 95..99@95 T:List) [ (Ident \
         100..101@100 T:T)])]) (Ident 106..109@106 t:???)) (ExtMethods 110..149@110 [ [ (ValDef 121..130@121 (Mods \
         [<param>] [] []) t:s (Ident 124..130@124 T:String) _)]] [ (DefDef 132..149@136 (Mods [<method>] [] []) \
         t:twice [] (TypeTree 141..141) (InfixOp 144..149@146 (Ident 144..145@144 t:s) (Ident 146..147@146 t:+) \
         (Ident 148..149@148 t:s)))])])"
    );
}

#[test]
fn imports_enums_placeholders_and_for_comprehensions() {
    let text = "import a.{b as c, *}\nenum Color { case Red, Green; case Rgb(r: Int) }\nval f = (_: Int) + 1\n\
                val r = for x <- xs if x > 0 yield x match { case 1 => \"a\" case _ => s\"b$x\" }\n";
    assert_eq!(
        parse(text),
        "(PackageDef 0..168 (Ident 0..0 t:<empty>) [ (Import 0..20@7 (Ident 7..8@7 t:a) [ (ImportSelector 10..16@12 \
         (Ident 10..11@10 t:b) (Ident 15..16@15 t:c) _) (ImportSelector 18..19@18 (Ident 18..19@18 t:_) _ _)]) \
         (TypeDef 21..69@26 (Mods [enum] [] []) T:Color (Template 34..67 (DefDef 34..34 (Mods [] [] []) t:<init> [] \
         (TypeTree 34..34) _) [] _self [ (PatDef 34..49@39 (Mods [case enum] [] []) [ (Ident 39..42@39 t:Red) \
         (Ident 44..49@44 t:Green)] (TypeTree 49..49) _) (TypeDef 51..67@56 (Mods [case enum] [] []) T:Rgb \
         (Template 59..67 (DefDef 59..67@59 (Mods [] [] []) t:<init> [ [ (ValDef 60..66@60 (Mods [<paramaccessor>] \
         [] []) t:r (Ident 63..66@63 T:Int) _)]] (TypeTree 66..66) _) [] _self []))])) (ValDef 70..90@74 (Mods [] [] \
         []) t:f (TypeTree 75..75) (WildcardFunction 78..90 [ (ValDef 79..85@79 (Mods [<param> <synthetic>] [] []) \
         t:_$1 (Ident 82..85@82 T:Int) _)] (InfixOp 78..90@87 (Parens 78..86@78 (Typed 79..85@79 (Ident 79..80@79 \
         t:_$1) (Ident 82..85@82 T:Int))) (Ident 87..88@87 t:+) (Number 89..90@89 \"1\" Whole(10))))) (ValDef \
         91..168@95 (Mods [] [] []) t:r (TypeTree 96..96) (ForYield 99..168@99 [ (GenFrom 103..110@105 (Ident \
         103..104@103 t:x) (Ident 108..110@108 t:xs) Check) (InfixOp 114..119@116 (Ident 114..115@114 t:x) (Ident \
         116..117@116 t:>) (Number 118..119@118 \"0\" Whole(10)))] (Match 126..168@128 (Ident 126..127@126 t:x) [ \
         (CaseDef 136..149@136 (Number 141..142@141 \"1\" Whole(10)) _ (Block 143..149@143 [] (Literal 146..149@146 \
         #String:\"a\"))) (CaseDef 150..166@150 (Ident 155..156@155 t:_) _ (Block 157..166@157 [] \
         (InterpolatedString 160..166@160 t:s [ (Thicket 162..165@162 [ (Literal 162..163@162 #String:\"b\") (Ident \
         164..165@164 t:x)]) (Literal 165..165@165 #String:\"\")])))])))])"
    );
}

#[test]
fn errors_recover_with_placeholder_trees() {
    assert_eq!(
        parse("val = 1\ndef g(x: Int =\n"),
        "(PackageDef 0..23 (Ident 0..0 t:<empty>) [ (PatDef 0..8@4 (Mods [] [] []) [ (Ident 3..8 t:_)] (TypeTree \
         7..7) (Select 7..8 (Select 7..7 (Select 7..7 (Ident 7..7 t:_root_) t:scala) t:Predef) t:???)) (DefDef \
         8..23@12 (Mods [<method>] [] []) t:g [ [ (ValDef 14..23@14 (Mods [<param>] [] []) t:x (Ident 17..20@17 \
         T:Int) (Select 22..23 (Select 22..22 (Select 22..22 (Ident 22..22 t:_root_) t:scala) t:Predef) t:???))]] \
         (TypeTree 22..22) (Select 22..23 (Select 22..22 (Select 22..22 (Ident 22..22 t:_root_) t:scala) t:Predef) \
         t:???))])\n\
         4 an identifier expected, but '=' found\n\
         23 expression expected but eof found\n\
         22 Missing return type"
    );
}

#[test]
fn deep_nesting_is_an_error_not_a_stack_overflow() {
    let cases = [
        ("val x = ", "(", "1", ")"),
        ("val x = ", "{", "1", "}"),
        ("val x: ", "List[", "Int", "]"),
        ("", "class A { ", "", "}"),
    ];
    for (prefix, open, inner, close) in cases {
        let text = format!("{prefix}{}{inner}{}", open.repeat(20_000), close.repeat(20_000));
        let out = parse(&text);
        assert!(out.contains("nesting is too deep"), "{open}: {}", &out[out.len().saturating_sub(300)..]);
    }
}

#[test]
fn language_imports_enable_features_for_their_scope() {
    let text = "object A:\n  import language.postfixOps\n  val x = xs size\nval y = xs size\n";
    assert_eq!(
        parse(text),
        "(PackageDef 0..73 (Ident 0..0 t:<empty>) [ (ModuleDef 0..56@7 (Mods [module] [] []) t:A (Template 12..56 \
         (DefDef 12..12 (Mods [] [] []) t:<init> [] (TypeTree 12..12) _) [] _self [ (Import 12..38@19 (Ident \
         19..27@19 t:language) [ (ImportSelector 28..38 (Ident 28..38@28 t:postfixOps) _ _)]) (ValDef 41..56@45 (Mods \
         [] [] []) t:x (TypeTree 46..46) (PostfixOp 49..56@57 (Ident 49..51@49 t:xs) (Ident 52..56@52 t:size)))])) \
         (ValDef 57..73@61 (Mods [] [] []) t:y (TypeTree 62..62) (InfixOp 65..73@68 (Ident 65..67@65 t:xs) (Ident \
         68..72@68 t:size) (Select 72..73 (Select 72..72 (Select 72..72 (Ident 72..72 t:_root_) t:scala) t:Predef) \
         t:???)))])\n\
         73 expression expected but eof found"
    );
}
