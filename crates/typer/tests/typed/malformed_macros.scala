import scala.quoted.*

object Malformed:
  inline def added(x: Int): Int = 1 + ${ twiceImpl('x) }
  inline def mapped(x: Int): Int = List(x).map(i => ${ twiceImpl('i) }).head
  inline def ascribed(x: Int): Int = (${ twiceImpl('x) }: Int)
  def twiceImpl(x: Expr[Int])(using Quotes): Expr[Int] = '{ $x + $x }
