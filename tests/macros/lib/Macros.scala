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

  inline def orNull(x: String): String = ${ orNullImpl('x, null, true, 2, "?") }
  def orNullImpl(x: Expr[String], fallback: String, on: Boolean, n: Int, suffix: String)(using
      Quotes
  ): Expr[String] =
    if fallback == null && on && n == 2 && suffix == "?" then '{ $x + "?" } else '{ "unexpected" }

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
