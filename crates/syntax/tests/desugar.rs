//! Desugaring on small inputs, against trees recorded from dotty 3.9.0 (the `desugar` mode of
//! `scripts/syntax-oracle`); `just syntax-report desugar` compares it on dotty's test suites.

use syntax::ast::print::Printer;
use syntax::config::Config;
use syntax::desugar::Desugar;
use syntax::names::Names;
use syntax::parse::Parser;
use syntax::source::SourceFile;

/// The desugared tree of `text` and its diagnostics, as `syntax-dump desugar` prints them.
fn desugar(text: &str, options: &[&str]) -> String {
    let source = SourceFile::new("t.scala", text);
    let mut names = Names::default();
    let mut p = Parser::new(&source, &mut names, Config::from_options(options));
    let root = p.parse();
    let cfg = p.s.cfg;
    let root = Desugar::new(&mut p.ast, p.s.names, &mut p.s.diags, &mut p.fresh, &source, cfg).unit(root);
    let mut printer = Printer { ast: &p.ast, names: p.s.names, out: String::new() };
    printer.tree(root);
    let mut out = printer.out;
    for d in p.s.diags.unique() {
        out.push_str(&format!("\n{} {}", d.pos(), d.message));
    }
    out
}

#[test]
fn pattern_definitions_split_into_variables() {
    let text =
        "class C:\n  val (a, b) = (1, 2)\n  var (c, d) = f()\n  lazy val (u, _) = g()\n  val Some(x) = h()\n";
    assert_eq!(
        desugar(text, &[]),
        "(PackageDef 0..93 (Ident 0..0 t:<empty>) [ (TypeDef 0..93@6 (Mods [] [] []) T:C (Template 11..93 \
         (DefDef 11..11 (Mods [] [] []) t:<init> [] (TypeTree 11..11) _) [] _self [ (ValDef 16..17@16 \
         @patternvar (Mods [] [] []) t:a (TypeTree 25..25) (Number 25..26@25 \"1\" Whole(10))) (ValDef \
         19..20@19 @patternvar (Mods [] [] []) t:b (TypeTree 28..28) (Number 28..29@28 \"2\" Whole(10))) \
         (ValDef 37..49@37 (Mods [private[this] <synthetic>] [] [ (Var 33..36@33)]) t:$2$ (TypeTree 37..37) \
         (Match 37..49 (Apply 46..49@47 @check=IrrefutablePatDef (Ident 46..47@46 t:f) []) [ (CaseDef 37..43 \
         (Bind 37..43 t:$1$ (Apply 37..43@37 (TypedSplice 38..38 t:scala.Tuple2) [ (Ident 38..39@38 t:c) \
         (Ident 41..42@41 t:d)])) _ (Ident 43..43 @forartifact t:$1$))])) (ValDef 38..39@38 @patternvar (Mods \
         [mutable] [] [ (Var 33..36@33)]) t:c (TypeTree 38..38) (Select 38..38 (Ident 38..38 t:$2$) t:_1)) \
         (DefDef 38..39@38 @patternvar (Mods [<accessor> mutable] [] [ (Var 33..36@33)]) t:c_= [ [ (ValDef \
         38..38 (Mods [<param> <synthetic>] [] []) t:x$1 (SetterParamTree 38..38 ~t:c) _)]] (TypedSplice \
         38..38 T:scala.Unit) (Literal 38..38 @syntheticunit #Unit)) (ValDef 41..42@41 @patternvar (Mods \
         [mutable] [] [ (Var 33..36@33)]) t:d (TypeTree 41..41) (Select 41..41 (Ident 41..41 t:$2$) t:_2)) \
         (DefDef 41..42@41 @patternvar (Mods [<accessor> mutable] [] [ (Var 33..36@33)]) t:d_= [ [ (ValDef \
         41..41 (Mods [<param> <synthetic>] [] []) t:x$1 (SetterParamTree 41..41 ~t:d) _)]] (TypedSplice \
         41..41 T:scala.Unit) (Literal 41..41 @syntheticunit #Unit)) (ValDef 61..73@61 (Mods [private[this] \
         lazy <synthetic>] [] [ (Lazy 52..56@52)]) t:$4$ (TypeTree 61..61) (Match 61..73 (Apply 70..73@71 \
         @check=IrrefutablePatDef (Ident 70..71@70 t:g) []) [ (CaseDef 61..67 (Bind 61..67 t:$3$ (Apply \
         61..67@61 (TypedSplice 62..62 t:scala.Tuple2) [ (Ident 62..63@62 t:u) (Ident 65..66@65 t:_)])) _ \
         (Ident 67..67 @forartifact t:$3$))])) (DefDef 62..63@62 @patternvar (Mods [] [] [ (Lazy 52..56@52)]) \
         t:u [] (TypeTree 62..62) (Select 62..62 (Ident 62..62 t:$4$) t:_1)) (ValDef 76..93@85 @patternvar \
         (Mods [] [] []) t:x (TypeTree 80..80) (Match 80..93 (Apply 90..93@91 @check=IrrefutablePatDef (Ident \
         90..91@90 t:h) []) [ (CaseDef 80..87 (Apply 80..87@84 (Ident 80..84@80 t:Some) [ (Ident 85..86@85 \
         t:x)]) _ (Ident 87..87 @forartifact t:x))]))]))])"
    );
}

#[test]
fn operators_tuples_and_interpolations_become_calls() {
    let text = "def m = (s\"a $x ${y}b\", x +: xs, !p, (1, 2), f(a :: Nil), (u, v) match { case (1, _) => })\n\
                def t(xs: Int*): (Int, String) = ???\n";
    assert_eq!(
        desugar(text, &[]),
        "(PackageDef 0..127 (Ident 0..0 t:<empty>) [ (DefDef 0..90@4 (Mods [<method>] [] []) t:m [] (TypeTree \
         5..5) (Apply 8..90@8 (TypedSplice 9..9 t:scala.Tuple6) [ (Apply 9..22@9 (Select 9..22@9 (Apply \
         11..21 (Select 11..11 (Select 11..11 (Ident 11..11 t:_root_) t:scala) t:StringContext) [ (Literal \
         11..13@11 #String:\"a \") (Literal 15..16@15 #String:\" \") (Literal 20..21@20 #String:\"b\")]) t:s) \
         [ (Ident 14..15@14 t:x) (Ident 18..19@18 t:y)]) (Apply 24..31 @infix (Select 26..31 (Ident 29..31@29 \
         t:xs) t:+:) [ (Ident 24..25@24 t:x)]) (Select 33..35@33 (Ident 34..35@34 t:p) t:unary_!) (Apply \
         37..43@37 (TypedSplice 38..38 t:scala.Tuple2) [ (Number 38..39@38 \"1\" Whole(10)) (Number 41..42@41 \
         \"2\" Whole(10))]) (Apply 45..56@46 (Ident 45..46@45 t:f) [ (Apply 47..55 @infix (Select 49..55 \
         (Ident 52..55@52 t:Nil) t:::) [ (Ident 47..48@47 t:a)])]) (Match 58..89@65 (Apply 58..64@58 \
         (TypedSplice 59..59 t:scala.Tuple2) [ (Ident 59..60@59 t:u) (Ident 62..63@62 t:v)]) [ (CaseDef \
         73..87@73 (Apply 78..84@78 (TypedSplice 79..79 t:scala.Tuple2) [ (Number 79..80@79 \"1\" Whole(10)) \
         (Ident 82..83@82 t:_)]) _ (Block 85..87@85 [] (Literal 85..87@85 @syntheticunit #Unit)))])])) \
         (DefDef 91..127@95 (Mods [<method>] [] []) t:t [ [ (ValDef 97..105@97 (Mods [<param>] [] []) t:xs \
         (Annotated 101..105@101 (AppliedTypeTree 101..104 (TypedSplice 101..101 \
         T:scala.collection.immutable.Seq) [ (Ident 101..104@101 T:Int)]) (Apply 104..104 (Select 104..104 \
         (New 104..104 (TypedSplice 104..104 T:scala.annotation.internal.Repeated)) t:<init>) [])) _)]] \
         (AppliedTypeTree 108..121@108 (TypedSplice 109..109 T:scala.Tuple2) [ (Ident 109..112@109 T:Int) \
         (Ident 114..120@114 T:String)]) (Ident 124..127@124 t:???))])"
    );
}

#[test]
fn for_with_alias_and_guard_before_3_8_tuples_the_alias() {
    let text = "def m = for\n  x <- xs\n  y = x + 1\n  if y > 2\n  (a, b) <- ps\nyield a\n";
    assert_eq!(
        desugar(text, &["-source", "3.7"]),
        "(PackageDef 0..67 (Ident 0..0 t:<empty>) [ (DefDef 0..67@4 (Mods [<method>] [] []) t:m [] (TypeTree \
         5..5) (Apply 8..67@8 (Select 14..44 (Apply 14..44 (Select 14..33 (Apply 14..33 (Select 19..21 (Ident \
         19..21@19 t:xs) t:map) [ (Function 14..33 [ (ValDef 14..15@14 @patternvar (Mods [<param>] [] []) t:x \
         (TypeTree 14..14) _)] (Block 14..33 [ (ValDef 24..33@24 @patternvar (Mods [] [] []) t:y (TypeTree \
         28..28) (Apply 28..33 @infix (Select 28..31@30 (Ident 28..29@28 t:x) t:+) [ (Number 32..33@32 \"1\" \
         Whole(10))]))] (Apply 14..25 @forartifact (TypedSplice 14..14 t:scala.Tuple2) [ (Ident 14..15@14 \
         t:x) (Ident 24..25@24 t:y)])))]) t:withFilter) [ (Function 14..44 [ (ValDef 14..14 (Mods [<param> \
         <synthetic>] [] []) t:x$1 (TypeTree 14..14) _)] (Match 14..44 (Annotated 14..14 (Ident 14..14 t:x$1) \
         (New 14..14 (TypedSplice 14..14 T:scala.unchecked))) [ (CaseDef 14..44 (Apply 14..25 (TypedSplice \
         14..14 t:scala.Tuple2) [ (Ident 14..15@14 t:x) (Ident 24..25@24 t:y)]) _ (Apply 39..44 @infix \
         (Select 39..42@41 (Ident 39..40@39 t:y) t:>) [ (Number 43..44@43 \"2\" Whole(10))]))]))]) t:flatMap) \
         [ (Function 14..67 [ (ValDef 14..14 (Mods [<param> <synthetic>] [] []) t:x$1 (TypeTree 14..14) _)] \
         (Match 14..67 (Annotated 14..14 (Ident 14..14 t:x$1) (New 14..14 (TypedSplice 14..14 \
         T:scala.unchecked))) [ (CaseDef 14..67 (Apply 14..25 (TypedSplice 14..14 t:scala.Tuple2) [ (Ident \
         14..15@14 t:x) (Ident 24..25@24 t:y)]) _ (Apply 47..67 (Select 57..59 (Ident 57..59@57 t:ps) t:map) \
         [ (Function 47..67 [ (ValDef 47..47 (Mods [<param> <synthetic>] [] []) t:x$1 (TypeTree 47..47) _)] \
         (Match 47..67 (Ident 47..47 @check=IrrefutableGenFrom t:x$1) [ (CaseDef 47..67 (Apply 47..53@47 \
         (TypedSplice 48..48 t:scala.Tuple2) [ (Ident 48..49@48 t:a) (Ident 51..52@51 t:b)]) _ (Ident \
         66..67@66 t:a))]))]))]))]))])"
    );
}

#[test]
fn type_quotes_hold_types_and_preview_enables_better_fors() {
    let text = "object O:\n  def f(using Quotes) = '[ A | (B, C) ]\n  def g = for x <- xs; y = x yield y\n";
    assert_eq!(
        desugar(text, &["-source:3.7", "-preview"]),
        "(PackageDef 0..86 (Ident 0..0 t:<empty>) [ (ModuleDef 0..86@7 (Mods [module] [] []) t:O (Template \
         12..86 (DefDef 12..12 (Mods [] [] []) t:<init> [] (TypeTree 12..12) _) [] _self [ (DefDef 12..49@16 \
         (Mods [<method>] [] []) t:f [ [ (ValDef 24..30 (Mods [<param> <synthetic> given] [] []) t:x$1 (Ident \
         24..30@24 T:Quotes) _)]] (TypeTree 31..31) (Quote 34..49@34 (AppliedTypeTree 37..47@39 (Ident \
         39..40@39 T:|) [ (Ident 37..38@37 T:A) (AppliedTypeTree 41..47@41 (TypedSplice 42..42 \
         T:scala.Tuple2) [ (Ident 42..43@42 T:B) (Ident 45..46@45 T:C)])]) [])) (DefDef 52..86@56 (Mods \
         [<method>] [] []) t:g [] (TypeTree 57..57) (Apply 60..86@60 (Select 69..71 (Ident 69..71@69 t:xs) \
         t:map) [ (Function 64..86 [ (ValDef 64..65@64 @patternvar (Mods [<param>] [] []) t:x (TypeTree \
         64..64) _)] (Block 73..86 [ (ValDef 73..78@73 @patternvar (Mods [] [] []) t:y (TypeTree 77..77) \
         (Ident 77..78@77 t:x))] (Ident 85..86@85 t:y)))]))]))])"
    );
}
