import scala.quoted.*

inline def power(x: Double, inline n: Int): Double = ${ powerCode('x, 'n) }

def powerCode(x: Expr[Double], n: Expr[Int])(using Quotes): Expr[Double] =
  n.value match
    case Some(0) => '{ 1.0 }
    case Some(k) => '{ $x * ${ powerCode(x, Expr(k - 1)) } }
    case None => '{ Math.pow($x, $n.toDouble) }

def typeName[T: Type](using Quotes): Expr[String] =
  val t = Type.show[T]
  '{ ${ Expr(t) } + "" }

def matchQuote(e: Expr[Any])(using Quotes): Expr[Int] = e match
  case '{ ($a: Int) + ($b: Int) } => '{ $a * $b }
  case '{ List[t]($xs*) } => '{ 0 }
  case _ => '{ -1 }

def isInt(t: Type[?])(using Quotes): Boolean = t match
  case '[Int] => true
  case '[List[t]] => false
  case _ => false

transparent inline def pick(inline b: Boolean): Any = inline if b then 1 else "one"
inline def dispatch[T](x: T): String = inline x match
  case _: Int => "int"
  case _ => "other"
