//> using scala 3.9.0
class Counter(val k: Int):
  def add(x: Int): Int = x + k
  def curried(x: Int)(y: Int): Int = x * y + k

object Values:
  var n = 0
  def next(): Int = { n += 1; n }
  def mk(): Counter = new Counter(next())
  def sum(a: Int, b: Int): Int = a + b
  def three(a: Int)(b: Int)(c: Int): Int = a + b + c
  def unit() = println("unit")

@main def methodvalues(): Unit =
  val add = Values.mk().add
  val partial = Values.mk().curried(Values.next())
  val curried: Int => Int => Int = Values.mk().curried
  val underscore = Values.sum _
  val nullary: () => Unit = Values.unit _
  val nested = Values.three(1)
  val chain = { println("block"); Values.mk() }.add
  println(add(1))
  println(partial(2))
  println(curried(3)(4))
  println(underscore(5, 6))
  println(nested(7)(8))
  println(chain(9))
  nullary()
  println(new Counter(1).getClass.getName)
  println(5.getClass)
