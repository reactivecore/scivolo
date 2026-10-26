import scala.reflect.Selectable.reflectiveSelectable

object Ta26StructuralCalls:
  type Calc = {
    def add(x: Int)(y: Int): Int
    def scaled(x: Int)(implicit factor: Int): Int
  }
  def use(c: Calc): Int =
    implicit val factor: Int = 3
    val f = c.add(1)
    val g = c.add(2)(_)
    c.add(1)(2) + c.scaled(4) + f(5) + g(6)
  def exampleFn(s: String, i: Int): Unit = ()
  def arity(f: (?, ?) => Any): Int = 2
  val n = arity(exampleFn(_, _))
