//> using scala 3.9.0
// Synthetic members of case classes without fields, and `&&` / `||` on boolean literals.

case class Plain()
case class One(a: Int)
trait Colors { case object Red }
class Palette extends Colors

object Conds {
  def loud(): Boolean = { println("evaluated"); true }
  def both(b: Boolean) = true && b
  def never(b: Boolean) = false && loud()
  def always(b: Boolean) = true || loud()
  def either(b: Boolean) = false || b
}

object Test {
  def element(p: Product, n: Int): String =
    try p.productElement(n).toString
    catch { case e: IndexOutOfBoundsException => "out of bounds: " + e.getMessage }
  def run(): Unit = {
    println(Plain() == Plain())
    println(Plain().hashCode == Plain().hashCode)
    println(Plain())
    println(element(Plain(), 0))
    println(element(One(4), 0))
    println(element(One(4), 1))
    println(element(new Palette().Red, 2))
    println(Conds.both(true))
    println(Conds.never(true))
    println(Conds.always(false))
    println(Conds.either(false))
  }
}

@main def emptycases(): Unit = Test.run()
