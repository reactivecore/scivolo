package dotty.tools.dotc.typeroracle

import dotty.tools.dotc.ast.tpd.*
import dotty.tools.dotc.core.Contexts.*
import dotty.tools.dotc.core.Flags.*
import dotty.tools.dotc.core.NameKinds.DefaultGetterName
import dotty.tools.dotc.core.Symbols.*
import dotty.tools.dotc.core.Types.*
import dotty.tools.dotc.util.Spans.Span
import scala.collection.mutable

/** Feature counts over a typed tree: what a file asks of a typer. */
class Stats(using Context) extends TreeTraverser:
  val counts = mutable.Map.empty[String, Int].withDefaultValue(0)
  def bump(k: String): Unit = counts(k) += 1

  def zeroWidth(s: Span): Boolean = !s.exists || s.isSynthetic || s.start == s.end

  def typeFeatures(tp: Type): Unit =
    var or, and, mt, refined, hk = false
    tp.existsPart({
      case _: OrType => or = true; false
      case _: AndType => and = true; false
      case _: MatchType => mt = true; false
      case _: RefinedType => refined = true; false
      case _: HKTypeLambda => hk = true; false
      case _ => false
    }, forceLazy = false)
    if or then bump("ty_union")
    if and then bump("ty_intersection")
    if mt then bump("ty_match")
    if refined then bump("ty_refined")
    if hk then bump("ty_hk_lambda")

  def traverse(tree: Tree)(using Context): Unit =
    bump("nodes")
    tree match
      case Inlined(call, _, _) if !call.isEmpty =>
        bump(if call.symbol.is(Macro) then "macro_calls" else "inline_expansions")
      case Apply(fun, args) =>
        fun.tpe.widen match
          case mt: MethodType if mt.isImplicitMethod =>
            bump(if args.forall(a => zeroWidth(a.span)) then "using_args_inferred" else "using_args_explicit")
          case _ =>
        if args.length == 1 && zeroWidth(fun.span) && fun.symbol.is(Method)
           && (fun.symbol.isOneOf(Implicit | Given) || fun.symbol.name.toString == "apply" && fun.symbol.owner.derivesFrom(defn.ConversionClass))
        then bump("conversions")
      case TypeApply(fun, args) =>
        bump(if args.forall(a => a.isInstanceOf[TypeTree] && zeroWidth(a.span)) then "type_args_inferred" else "type_args_explicit")
      case _: Closure => bump("closures")
      case _: Match => bump("matches")
      case _: UnApply => bump("unapplies")
      case _: NamedArg => bump("named_args")
      case _: SeqLiteral => bump("seq_literals")
      case _: Try => bump("tries")
      case _: Return => bump("returns")
      case _: Quote | _: Splice | _: QuotePattern | _: SplicePattern => bump("quotes")
      case _: LambdaTypeTree => bump("tt_type_lambda")
      case _: MatchTypeTree => bump("tt_match_type")
      case _: SingletonTypeTree => bump("tt_singleton")
      case _: RefinedTypeTree => bump("tt_refined")
      case _: Import => bump("imports")
      case _: Export => bump("exports")
      case t: TypeDef if t.symbol.is(Opaque) => bump("opaque_defs")
      case t: TypeDef if t.symbol.isClass && t.symbol.is(Enum) => bump("enum_classes")
      case t: MemberDef if t.symbol.isOneOf(Given | Implicit) && !t.symbol.is(Param) => bump("given_defs")
      case t: DefDef if t.symbol.is(Inline) => bump("inline_defs")
      case t: DefDef if t.symbol.is(ExtensionMethod) => bump("extension_defs")
      case t: ValDef if t.symbol.is(Lazy) => bump("lazy_vals")
      case _ =>
    tree match
      case t: RefTree if t.symbol.exists =>
        val s = t.symbol
        if s.is(ExtensionMethod) then bump("extension_calls")
        if s.is(JavaDefined) then bump("java_refs")
        if s.name.is(DefaultGetterName) then bump("default_args")
        if s.is(Method) && s.owner.isClass && s.owner.info.member(s.name).isOverloaded then bump("overloaded_refs")
        if s.is(Method) && s.info.isInstanceOf[PolyType] then bump("poly_method_refs")
      case _ =>
    tree match
      case t: TypeTree if t.hasType => bump("type_trees"); if zeroWidth(t.span) then bump("type_trees_inferred"); typeFeatures(t.tpe)
      case t if t.isTerm && t.hasType => typeFeatures(t.tpe)
      case _ =>
    traverseChildren(tree)
