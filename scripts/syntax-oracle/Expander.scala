package dotty.tools.dotc.syntaxoracle

import dotty.tools.dotc.*
import dotty.tools.dotc.ast.{Trees, untpd, desugar}
import dotty.tools.dotc.core.Contexts.*
import dotty.tools.dotc.core.Mode
import dotty.tools.dotc.core.Flags.*
import dotty.tools.dotc.core.Names.Name
import dotty.tools.dotc.core.NameOps.*
import dotty.tools.dotc.core.Scopes.newScope
import dotty.tools.dotc.core.StdNames.{nme, tpnme}
import dotty.tools.dotc.core.Symbols.*
import dotty.tools.dotc.core.Types.*

/** How much of the desugaring `Expander` replays: expressions and pattern definitions; then
 *  other definitions; then classes, objects and companions; then enums.
 */
enum Stage:
  case Exprs, Defs, Classes, All

/** Replays dotty's pre-typing desugaring over a parsed compilation unit:
 *   - statement lists are expanded as `Namer.index(stats)` does (`desugar.defTree`, `packageDef`,
 *     `extMethods`, then the merge of synthetic and explicit companions), under an owner that is a
 *     package, a class (with a module class linked to its companion, and type parameters for enums)
 *     or a term, as Namer's contexts are;
 *   - expressions, types and patterns are rewritten as `Typer.typedUnadapted` does before typing:
 *     `desugar(tree)` for `ForYield`/`ForDo`/`InterpolatedString`/`SymbolLit`/`PostfixOp`/
 *     `PrefixOp`/`Parens`, `desugar.binop` for infix expressions, `desugar.tuple` for unnamed
 *     tuples, `desugar.block`, with `Mode.Type`/`Mode.Pattern` tracked like the typer does.
 *  Left alone (they need expected types): lambdas, `{ case ... }` blocks, named tuples, assignments.
 */
class Expander(stage: Stage = Stage.Exprs) extends untpd.UntypedTreeMap:
  import untpd.*

  private val classSyms = new java.util.IdentityHashMap[TypeDef, ClassSymbol]

  private def typeCtx(using Context): Context = ctx.fresh.setMode(Mode.Type)
  private def exprCtx(using Context): Context = ctx.fresh.setMode(Mode.None)
  private def patCtx(using Context): Context = ctx.fresh.setMode(Mode.Pattern)
  private def termOwner(name: Name, flags: FlagSet)(using Context): Context =
    ctx.fresh.setOwner(newSymbol(ctx.owner, name.toTermName, flags.toTermFlags & Method, NoType))

  /** `Namer.index(unit.untpdTree)`: the compilation unit is itself a statement to expand. */
  def unit(tree: Tree)(using Context): Tree = transformStats(tree :: Nil, NoSymbol) match
    case t :: Nil => t
    case ts => Thicket(ts)

  private def classSym(td: TypeDef)(using Context): ClassSymbol =
    Option(classSyms.get(td)).getOrElse {
      val decls = newScope
      val cls = newCompleteClassSymbol(ctx.owner, td.name, td.mods.flags.toTypeFlags, Nil, decls)
      td.rhs match
        case impl: Template =>
          val accessors = impl.body.collect { case a: TypeDef if a.mods.is(Param) => a.name -> a }.toMap
          for tp <- impl.constr.leadingTypeParams do
            val variance = accessors.get(tp.name).fold(tp.mods.flags)(_.mods.flags) & VarianceFlags
            decls.enter(newSymbol(cls, tp.name, TypeParam | variance, TypeBounds.empty))
        case _ =>
      classSyms.put(td, cls)
      cls
    }

  override def transform(tree: Tree)(using Context): Tree = tree match
    case _: ForYield | _: ForDo | _: InterpolatedString | _: SymbolLit | _: PostfixOp | _: PrefixOp =>
      val t1 = desugar(tree)
      if t1 eq tree then super.transform(tree) else transform(t1)
    case Parens(t) => transform(t)
    case InfixOp(l, op, r) =>
      if ctx.mode.is(Mode.Type) then transform(cpy.AppliedTypeTree(tree)(op, l :: r :: Nil))
      else if ctx.mode.is(Mode.Pattern) then transform(cpy.Apply(tree)(op, l :: r :: Nil))
      else transform(desugar.binop(l, op, r).withAttachmentsFrom(tree))
    case tree: Tuple if !tree.trees.exists(_.isInstanceOf[NamedArg]) =>
      val t1 = desugar.tuple(tree, NoType).withAttachmentsFrom(tree)
      if t1 eq tree then super.transform(tree) else transform(t1)
    case blk: Block => transformBlock(desugar.block(blk))
    case Typed(expr, tpt) => cpy.Typed(tree)(transform(expr), transform(tpt)(using typeCtx))
    case New(impl: Template) =>
      val cls = newCompleteClassSymbol(ctx.owner, tpnme.ANON_CLASS, Final, Nil, newScope)
      cpy.New(tree)(transformTemplate(impl)(using ctx.fresh.setOwner(cls).setTree(tree)))
    case New(tpt) => cpy.New(tree)(transform(tpt)(using typeCtx))
    case TypeApply(fn, args) => cpy.TypeApply(tree)(transform(fn), transform(args)(using typeCtx))
    case Annotated(arg, annot) => cpy.Annotated(tree)(transform(arg), transform(annot)(using exprCtx))
    case SingletonTypeTree(ref) => cpy.SingletonTypeTree(tree)(transform(ref)(using exprCtx))
    case _: AppliedTypeTree | _: LambdaTypeTree | _: TypeBoundsTree | _: ByNameTypeTree | _: MatchTypeTree
        | _: RefinedTypeTree | _: ContextBounds | _: ContextBoundTypeTree | _: TermLambdaTypeTree =>
      super.transform(tree)(using typeCtx)
    case CaseDef(pat, guard, body) =>
      val pat1 = if ctx.mode.is(Mode.Type) then transform(pat) else transform(pat)(using patCtx)
      cpy.CaseDef(tree)(pat1, transform(guard)(using exprCtx), transform(body)(using exprCtx))
    case GenFrom(pat, expr, mode) => untpd.cpy.GenFrom(tree)(transform(pat)(using patCtx), transform(expr), mode)
    case GenAlias(pat, expr) => untpd.cpy.GenAlias(tree)(transform(pat)(using patCtx), transform(expr))
    case tree: ValDef if !tree.isEmpty =>
      given Context = termOwner(tree.name, tree.mods.flags)
      cpy.ValDef(tree)(tree.name, transform(tree.tpt)(using typeCtx), transform(tree.rhs)(using exprCtx))
    case tree: DefDef =>
      given Context = termOwner(tree.name, tree.mods.flags | Method)
      cpy.DefDef(tree)(tree.name, transformParamss(tree.paramss), transform(tree.tpt)(using typeCtx),
        transform(tree.rhs)(using exprCtx))
    case tree @ TypeDef(name, impl: Template) =>
      val cls = classSym(tree)
      cpy.TypeDef(tree)(name, transformTemplate(impl)(using ctx.fresh.setOwner(cls).setTree(tree)))
    case tree @ TypeDef(name, rhs) => cpy.TypeDef(tree)(name, transform(rhs)(using typeCtx))
    case tree: Template => transformTemplate(tree)
    case _ => super.transform(tree)

  private def transformTemplate(impl: Template)(using Context): Template =
    def parent(p: Tree): Tree = p match
      case _: Apply | _: TypeApply | _: Select | _: New => transform(p)(using exprCtx)
      case _ => transform(p)(using typeCtx)
    val self = if impl.self.isEmpty then impl.self
      else cpy.ValDef(impl.self)(impl.self.name, transform(impl.self.tpt)(using typeCtx), impl.self.rhs)
    cpy.Template(impl)(transformSub(impl.constr), impl.parents.mapConserve(parent),
      transform(impl.derived)(using typeCtx), self, transformStats(impl.body, NoSymbol))

  /** Whether `Namer.index` expands `stat` at the current stage. */
  private def expands(stat: Tree)(using Context): Boolean = stat match
    case t: PatDef => !t.mods.isEnumCase || stage == Stage.All
    case t: TypeDef if t.isClassDef =>
      if t.mods.is(Enum) then stage == Stage.All else stage.ordinal >= Stage.Classes.ordinal
    case _: ModuleDef => stage.ordinal >= Stage.Classes.ordinal
    case _: DefTree | _: PackageDef | _: ExtMethods => stage.ordinal >= Stage.Defs.ordinal
    case _ => false

  /** `Namer.index(stats)`: expand, merge companions, create class symbols, recurse into the expansions. */
  override def transformStats(stats: List[Tree], exprOwner: Symbol)(using Context): List[Tree] =
    val expanded = stats.map {
      case t if !expands(t) => t
      case t: DefTree => desugar.defTree(t)
      case t: PackageDef => desugar.packageDef(t)
      case t: ExtMethods => desugar.extMethods(t)
      case t => t
    }
    val flat = Trees.flatten(if stage.ordinal >= Stage.Classes.ordinal then mergeCompanions(expanded) else expanded)
    val classes = flat.collect { case td: TypeDef if td.isClassDef => td }
    val byName = classes.groupBy(_.name)
    for td <- classes do classSym(td)
    for td <- classes if td.mods.is(ModuleClass) do
      for ctd <- byName.getOrElse(td.name.stripModuleClassSuffix.toTypeName, Nil).headOption do
        classSym(td).registerCompanion(classSym(ctd))
        classSym(ctd).registerCompanion(classSym(td))
    flat.mapConserve(transform)

  /** `Namer.mergeCompanionDefs`: a synthetic companion (of a case class, enum, value class or
   *  `derives` clause) is folded into an explicit object of the same name.
   */
  private def mergeCompanions(expanded: List[Tree])(using Context): List[Tree] =
    val groups = expanded.map { case Thicket(ts) => ts.toBuffer; case t => List(t).toBuffer }
    def modClasses = for (g, i) <- groups.zipWithIndex; t <- g.toList collect { case td: TypeDef if td.mods.is(ModuleClass) => td }
      yield (i, t)
    for (i, syn) <- modClasses if syn.mods.is(Synthetic) do
      modClasses.find((j, t) => j != i && t.name == syn.name && !t.mods.is(Synthetic)) match
        case Some((j, expl)) =>
          val synTempl = syn.rhs.asInstanceOf[Template]
          val explTempl = expl.rhs.asInstanceOf[Template]
          val merged = cpy.TypeDef(expl)(rhs = cpy.Template(explTempl)(
            derived = if synTempl.derived.nonEmpty then synTempl.derived else explTempl.derived,
            body = synTempl.body.filter {
              case stat: DefDef => stat.name != nme.toString_
              case _ => true
            } ++ explTempl.body))
          merged.setMods(expl.mods)
          if synTempl.derived.nonEmpty then merged.putAttachment(desugar.DerivingCompanion, synTempl.srcPos.startPos)
          groups(j)(groups(j).indexOf(expl)) = merged
          groups(i) -= syn
          groups(i).indexWhere { case vd: ValDef => vd.name == syn.name.sourceModuleName && vd.mods.is(Synthetic); case _ => false } match
            case -1 =>
            case k => groups(i).remove(k)
        case None =>
    groups.flatMap(_.toList).toList
