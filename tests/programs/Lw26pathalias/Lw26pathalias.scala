abstract class GenericProduct[P]:
  type Prod = P
  def toProduct(x: P): Product

class Pair(val a: Int, val b: Int) extends Product:
  def canEqual(that: Any) = true
  def productArity = 2
  def productElement(n: Int): Any = if n == 0 then a else b

object PairGeneric extends GenericProduct[Pair]:
  def toProduct(x: Pair): Product = x

def show[T](x: T, cas: GenericProduct[? <: T]): Int =
  cas.toProduct(x.asInstanceOf[cas.Prod]).productArity

@main def Lw26pathalias(): Unit =
  println(show[Pair](new Pair(1, 2), PairGeneric))
