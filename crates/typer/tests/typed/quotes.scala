import scala.quoted.*

object Quoted:
  inline def twice(inline x: Int): Int = ${ twiceImpl('x) }
  def twiceImpl(x: Expr[Int])(using Quotes): Expr[Int] = '{ $x + $x }

  inline def pair[T](x: T): (T, T) = ${ pairImpl('x) }
  def pairImpl[T: Type](x: Expr[T])(using Quotes): Expr[(T, T)] = '{ ($x, $x) }

  def listed[T: Type](x: Expr[T])(using Quotes): Expr[List[T]] = typed('{ List($x) })(using Type.of[List[T]])
  def typed[U: Type](e: Expr[U])(using Quotes): Expr[U] = '{ identity[U]($e) }

  def nested(x: Expr[Int])(using Quotes): Expr[Int] = '{ val y = $x; ${ '{ y + 1 } } * 2 }
  def canceled(x: Expr[Int])(using Quotes): Expr[Int] = '{ ${ x } }
  def unquoted(x: Expr[Int])(using Quotes): Expr[Int] = '{ 1 + ${ '{ 2 } } }
  def stored(using Quotes): Quotes ?=> Expr[String] = '{ "s" }
