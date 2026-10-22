package macrolib

import scala.quoted.*

object Macros:
  inline def answer: Int = ${ answerImpl }
  def answerImpl(using Quotes): Expr[Int] = '{ 42 }

  inline def twice(inline x: Int): Int = ${ twiceImpl('x) }
  def twiceImpl(x: Expr[Int])(using Quotes): Expr[Int] = '{ $x + $x }

  inline def greet(name: String): String = ${ greetImpl('name) }
  def greetImpl(name: Expr[String])(using Quotes): Expr[String] = '{ "Hello, " + $name + "!" }

  inline def pair[T](x: T): (T, T) = ${ pairImpl('x) }
  def pairImpl[T: Type](x: Expr[T])(using Quotes): Expr[(T, T)] = '{ ($x, $x) }

  inline def listOf[T](x: T, y: T): List[T] = ${ listOfImpl('x, 'y) }
  def listOfImpl[T: Type](x: Expr[T], y: Expr[T])(using Quotes): Expr[List[T]] = '{ List[T]($x, $y) }

  inline def incremented(x: Int): Int = ${ incrementedImpl('x) }
  def incrementedImpl(x: Expr[Int])(using Quotes): Expr[Int] = '{ Helpers.inc($x) }

  inline def logged[T](x: T): T = ${ loggedImpl('x) }
  def loggedImpl[T: Type](x: Expr[T])(using Quotes): Expr[T] = '{ val v = $x; println(v); v }

  inline def plusOneTwice(inline x: Int): Int = ${ twiceImpl('{ x + 1 }) }

  inline def wrapped[T](x: T): List[T] = ${ wrappedImpl('x) }
  def wrappedImpl[T: Type](x: Expr[T])(using Quotes): Expr[List[T]] = typed('{ List($x) })(using Type.of[List[T]])
  def typed[U: Type](e: Expr[U])(using Quotes): Expr[U] = '{ identity[U]($e) }

  inline def fail: Int = ${ failImpl }
  def failImpl(using Quotes): Expr[Int] = throw new RuntimeException("the macro failed")

  inline def describe[T](x: T): String = ${ describeImpl('x) }
  def describeImpl[T: Type](x: Expr[T])(using Quotes): Expr[String] = x match
    case '{ $i: Int }    => '{ "an Int: " + $i }
    case '{ $s: String } => '{ "a String: " + $s }
    case _               => Expr("something else")

  inline def elemOf[T](x: T): String = ${ elemOfImpl('x) }
  def elemOfImpl[T: Type](x: Expr[T])(using Quotes): Expr[String] = x match
    case '{ $l: List[e] } => '{ $l.headOption.toString }
    case _                => '{ "no list" }

  inline def plusOne(inline n: Int): Int = ${ plusOneImpl('n) }
  def plusOneImpl(n: Expr[Int])(using Quotes): Expr[Int] =
    import quotes.reflect.*
    Expr(n.asTerm.underlying.asInstanceOf[Literal].constant.value.asInstanceOf[Int] + 1)

  inline def firstDefault[T]: Option[Any] = ${ firstDefaultImpl[T] }
  def firstDefaultImpl[T: Type](using Quotes): Expr[Option[Any]] =
    import quotes.reflect.*
    val companion = TypeRepr.of[T].typeSymbol.companionModule
    companion.declaredMethod("$lessinit$greater$default$1").headOption match
      case Some(s) => '{ Some(${ Ref(companion).select(s).asExpr }) }
      case None    => Expr(None)

  inline def typeArgCount[T]: Int = ${ typeArgCountImpl[T] }
  def typeArgCountImpl[T: Type](using Quotes): Expr[Int] =
    import quotes.reflect.*
    Expr(TypeRepr.of[T].typeArgs.size)

  inline def tupled(a: Any, b: Any): Tuple = ${ tupledImpl('a, 'b) }
  def tupledImpl(a: Expr[Any], b: Expr[Any])(using Quotes): Expr[Tuple] = Expr.ofTupleFromSeq(Seq(a, b))

object Source:
  inline def line: Int = ${ lineImpl }
  def lineImpl(using Quotes): Expr[Int] = Expr(quotes.reflect.Position.ofMacroExpansion.startLine + 1)

  inline def location: String = ${ locationImpl }
  def locationImpl(using Quotes): Expr[String] =
    val pos = quotes.reflect.Position.ofMacroExpansion
    Expr(s"${pos.sourceFile.name}:${pos.startLine + 1}:${pos.startColumn}-${pos.endLine + 1}:${pos.endColumn}")

  inline def name: String = ${ nameImpl }
  def nameImpl(using Quotes): Expr[String] = Expr(simple(owner.name))

  inline def fullName: String = ${ fullNameImpl }
  def fullNameImpl(using Quotes): Expr[String] =
    Expr(owner.fullName.split("\\.").map(n => simple(n).stripPrefix("_$")).filterNot(synthetic).mkString("."))

  inline def enclosing: String = ${ enclosingImpl }
  def enclosingImpl(using Quotes): Expr[String] =
    import quotes.reflect.*
    val names = Iterator.iterate(owner)(_.owner).takeWhile(!_.isNoSymbol).map(s => simple(s.name)).toList
    Expr(names.reverse.filterNot(n => synthetic(n) || n == "<root>" || n == "<empty>").mkString("."))

  inline def text[T](inline x: T): String = ${ textImpl('x) }
  def textImpl[T](x: Expr[T])(using Quotes): Expr[String] =
    import quotes.reflect.*
    Expr(x.asTerm.pos.sourceCode.getOrElse("<no source>"))

  private def owner(using Quotes): quotes.reflect.Symbol =
    var s = quotes.reflect.Symbol.spliceOwner
    while synthetic(simple(s.name)) do s = s.owner
    s

  private def simple(name: String): String = name.trim.stripSuffix("$")

  private def synthetic(name: String): Boolean =
    name == "<init>" || name.startsWith("<local ") && name.endsWith(">") || name == "$anonfun" || name == "macro"

object Symbols:
  inline def flagsOf[T]: String = ${ flagsOfImpl[T] }
  def flagsOfImpl[T: Type](using Quotes): Expr[String] =
    import quotes.reflect.*
    Expr(describe(TypeRepr.of[T].typeSymbol))

  inline def owners: String = ${ ownersImpl }
  def ownersImpl(using Quotes): Expr[String] =
    import quotes.reflect.*
    val root = Set(defn.RootPackage, defn.RootClass)
    val chain = Iterator.iterate(Symbol.spliceOwner)(_.owner).takeWhile(s => !root(s))
    Expr(chain.map(describe).mkString(" < "))

  private def describe(using Quotes)(s: quotes.reflect.Symbol): String =
    import quotes.reflect.*
    val flags = List(
      "Abstract" -> Flags.Abstract, "Case" -> Flags.Case, "Enum" -> Flags.Enum, "Final" -> Flags.Final,
      "Given" -> Flags.Given, "Implicit" -> Flags.Implicit, "Inline" -> Flags.Inline, "Lazy" -> Flags.Lazy,
      "Macro" -> Flags.Macro, "Method" -> Flags.Method, "Module" -> Flags.Module, "Mutable" -> Flags.Mutable,
      "Package" -> Flags.Package, "Param" -> Flags.Param, "Private" -> Flags.Private, "Sealed" -> Flags.Sealed,
      "Synthetic" -> Flags.Synthetic, "Trait" -> Flags.Trait, "Scala2x" -> Flags.Scala2x, "JavaDefined" -> Flags.JavaDefined
    ).collect { case (n, f) if s.flags.is(f) => n }
    val kinds = List(
      "package" -> s.isPackageDef, "class" -> s.isClassDef, "type" -> s.isTypeDef, "val" -> s.isValDef,
      "def" -> s.isDefDef, "term" -> s.isTerm, "constructor" -> s.isClassConstructor,
      "dummy" -> s.isLocalDummy, "moduleClass" -> (s.moduleClass != Symbol.noSymbol)
    ).collect { case (n, true) => n }
    s"${s.name}: ${kinds.mkString(" ")}; ${flags.mkString(" ")}"

object Terms:
  inline def shape(inline x: Any): String = ${ shapeImpl('x) }
  def shapeImpl(x: Expr[Any])(using Quotes): Expr[String] =
    import quotes.reflect.*
    Expr(show(x.asTerm))

  inline def shapeUnderlying(inline x: Any): String = ${ shapeUnderlyingImpl('x) }
  def shapeUnderlyingImpl(x: Expr[Any])(using Quotes): Expr[String] =
    import quotes.reflect.*
    Expr(show(x.asTerm.underlyingArgument))

  inline def known(inline x: Int): String = ${ knownImpl('x) }
  def knownImpl(x: Expr[Int])(using Quotes): Expr[String] = Expr(x.value.fold("unknown")(v => s"known $v"))

  inline def knownText(inline x: String): String = ${ knownTextImpl('x) }
  def knownTextImpl(x: Expr[String])(using Quotes): Expr[String] = Expr(x.valueOrAbort.reverse)

  inline def count(inline xs: Int*): Int = ${ countImpl('xs) }
  def countImpl(xs: Expr[Seq[Int]])(using Quotes): Expr[Int] = xs match
    case Varargs(es) => Expr(es.size)
    case _           => Expr(-1)

  inline def segments(s: String): Array[String] = ${ segmentsImpl('s) }
  def segmentsImpl(s: Expr[String])(using Quotes): Expr[Array[String]] =
    import quotes.reflect.*
    s.asTerm match
      case Inlined(_, _, Literal(StringConstant(l))) => Expr(l.split('/'))
      case _                                         => '{ $s.split('/') }

  inline def words(inline s: String): List[String] = ${ wordsImpl('s) }
  def wordsImpl(s: Expr[String])(using Quotes): Expr[List[String]] = Expr(s.valueOrAbort.split(' ').toList)

  private def show(using Quotes)(t: quotes.reflect.Tree): String =
    import quotes.reflect.*
    def all(ts: List[Tree]) = ts.map(show).mkString(", ")
    t match
      case Inlined(call, bindings, e) => s"Inlined(${call.isDefined}, [${all(bindings)}], ${show(e)})"
      case Literal(c)                 => s"Literal(${c.value})"
      case Typed(e, tpt)              => s"Typed(${show(e)}, ${show(tpt)})"
      case Block(stats, e)            => s"Block([${all(stats)}], ${show(e)})"
      case Apply(f, args)             => s"Apply(${show(f)}, [${all(args)}])"
      case TypeApply(f, args)         => s"TypeApply(${show(f)}, [${all(args)}])"
      case Select(q, n)               => s"Select(${show(q)}, $n)"
      case Ident(n)                   => s"Ident($n)"
      case Repeated(es, tpt)          => s"Repeated([${all(es)}], ${show(tpt)})"
      case NamedArg(n, a)             => s"NamedArg($n, ${show(a)})"
      case ValDef(n, tpt, rhs)        => s"ValDef($n, ${show(tpt)}, ${rhs.map(show)})"
      case Inferred()                 => "Inferred"
      case tpt: TypeTree              => s"TypeTree(${tpt.tpe.typeSymbol.name})"
      case t: Term                    => "Term"
      case _                          => "Tree"

object Checks:
  inline def positive(inline x: Int): Int = ${ positiveImpl('x) }
  def positiveImpl(x: Expr[Int])(using Quotes): Expr[Int] =
    import quotes.reflect.*
    x.value match
      case Some(v) if v < 0 => report.errorAndAbort(s"negative: $v", x)
      case Some(v)          => Expr(v)
      case None             => report.errorAndAbort("not a constant")

  inline def flagged(inline x: Int): Int = ${ flaggedImpl('x) }
  def flaggedImpl(x: Expr[Int])(using Quotes): Expr[Int] =
    import quotes.reflect.*
    report.error("flagged", x.asTerm.pos)
    x

  inline def sameType[A, B]: Boolean = ${ sameTypeImpl[A, B] }
  def sameTypeImpl[A: Type, B: Type](using Quotes): Expr[Boolean] =
    import quotes.reflect.*
    Expr(TypeRepr.of[A] =:= TypeRepr.of[B])

  inline def subType[A, B]: Boolean = ${ subTypeImpl[A, B] }
  def subTypeImpl[A: Type, B: Type](using Quotes): Expr[Boolean] =
    import quotes.reflect.*
    Expr(TypeRepr.of[A] <:< TypeRepr.of[B])

  inline def isInt(inline x: Any): Boolean = ${ isIntImpl('x) }
  def isIntImpl(x: Expr[Any])(using Quotes): Expr[Boolean] = Expr(x.isExprOf[Int])

  inline def asInt(inline x: Any): Int = ${ asIntImpl('x) }
  def asIntImpl(x: Expr[Any])(using Quotes): Expr[Int] = '{ ${ x.asExprOf[Int] } + 1 }

  inline def widened(inline x: Any): Boolean = ${ widenedImpl('x) }
  def widenedImpl(x: Expr[Any])(using Quotes): Expr[Boolean] =
    import quotes.reflect.*
    Expr(x.asTerm.tpe.widen =:= TypeRepr.of[Int])

class Chunk(val parts: List[String])

trait ChunkMacros:
  inline implicit def validated(s: String): Chunk = ${ Chunk.validatedImpl('s) }

object Chunk extends ChunkMacros:
  private[macrolib] implicit def internal(s: String): Chunk = Chunk(List(s))
  def validatedImpl(s: Expr[String])(using Quotes): Expr[Chunk] =
    import quotes.reflect.*
    s.asTerm match
      case Inlined(_, _, Literal(StringConstant(l))) => '{ Chunk(${ Expr(l.split('/').toList) }) }
      case _                                         => '{ Chunk(List($s)) }

object Tuples:
  import scala.compiletime.{constValue, erasedValue}

  inline def size[T <: Tuple]: Int = inline erasedValue[T] match
    case _: EmptyTuple => 0
    case _: (t *: ts)  => 1 + size[ts]

  inline def labels[T <: Tuple]: List[String] = inline erasedValue[T] match
    case _: EmptyTuple => Nil
    case _: (t *: ts)  => constValue[t].asInstanceOf[String] :: labels[ts]

  inline def sizeOf[T]: Int = constValue[Tuple.Size[T & Tuple]]

  inline def elemKind[L]: String = inline erasedValue[L] match
    case _: List[t] => kind[t]
    case _          => "no list"

  inline def kind[T]: String = inline erasedValue[T] match
    case _: Int => "Int or Nothing"
    case _      => "other"

object Helpers:
  inline def inc(x: Int): Int = x + 1
