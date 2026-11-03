import scala.quoted.*

object Macros:
  inline def describe[T](x: T): String = ${ describeImpl('x) }
  def describeImpl[T: Type](x: Expr[T])(using Quotes): Expr[String] = x match
    case '{ $i: Int }    => '{ "an Int: " + $i }
    case '{ $s: String } => '{ "a String: " + $s }
    case _               => Expr("something else")

  inline def elemOf[T](x: T): String = ${ elemOfImpl('x) }
  def elemOfImpl[T: Type](x: Expr[T])(using Quotes): Expr[String] = x match
    case '{ $l: List[e] } => '{ $l.headOption.toString }
    case _                => '{ "no list" }

  inline def nested(a: Any): Any = ${ nestedImpl('a) }
  def nestedImpl(a: Expr[Any])(using Quotes): Expr[Any] =
    import quotes.reflect.*
    a.asTerm.tpe.widen.asType match
      case '[t] =>
        '{ List.empty[t] }.asTerm.tpe.asType match
          case '[u] => '{ Option.empty[u] }

object Terms:
  inline def open(inline x: Int): String = ${ openImpl('x) }
  def openImpl(x: Expr[Int])(using Quotes): Expr[String] = x match
    case '{ val y: Int = 1; $body(y): Int }         => Expr(body.show)
    case '{ def g(y: Int): Int = $body(g, y); g(2) } => Expr(body.show)
    case _                                           => Expr("no")

object TypePatterns:
  inline def kind[T]: String = ${ kindImpl[T] }
  def kindImpl[T: Type](using Quotes): Expr[String] =
    import quotes.reflect.*
    def isInt[X: Type] = TypeRepr.of[X] =:= TypeRepr.of[Int]
    Type.of[T] match
      case '[List[t]]   => Expr(s"list of ${if isInt[t] then "Int" else "other"}")
      case '[Map[k, v]] => Expr(s"map ${isInt[k]} ${isInt[v]}")
      case '[Int]       => Expr("int")
      case _            => Expr("other")

  inline def emptyOf[T]: Any = ${ emptyOfImpl[T] }
  def emptyOfImpl[T: Type](using Quotes): Expr[Any] =
    import quotes.reflect.*
    TypeRepr.of[T].asType match
      case '[Option[t]] => '{ List.empty[t] }
      case '[t]         => '{ Option.empty[t] }

object LhReflect:
  inline def isOne(inline x: Int): String = ${ isOneImpl('x) }
  def isOneImpl(x: Expr[Int])(using Quotes): Expr[String] = x match
    case '{ 1 } => Expr("one")
    case _      => Expr("other")

object Traced:
  def wrap(e: Expr[Any])(using Quotes): Expr[Any] =
    e match
      case '{ $r: t } => '{ println("x"); $r }

object Applied:
  def f(e: Expr[Any])(using Quotes): Expr[Any] =
    e match
      case '{ Some($x: Int) }      => x
      case '{ ($a: t, $b: u) }     => '{ Map.empty[u, t] }
      case '{ Some($y: t) }        =>
        given Type[t] = Type.of[Int].asInstanceOf[Type[t]]
        '{ Option.empty[t] }
      case _                       => e

object Contra:
  def f(e: Expr[Any])(using Quotes): Expr[Any] =
    e match
      case '{ $g: (a => Int) } => g
      case _ => e
