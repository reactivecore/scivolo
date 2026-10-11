class Counter(val k: Int):
  def add(x: Int): Int = x + k
  def curried(x: Int)(y: Int): Int = x * y + k

object Main:
  var n = 0
  def next(): Int = { n += 1; n }
  def mk(): Counter = new Counter(next())
  def sum(a: Int, b: Int): Int = a + b
  def three(a: Int)(b: Int)(c: Int): Int = a + b + c
  def unit() = println(2)

  def run(): Unit =
    val add = mk().add
    val partial = mk().curried(next())
    val curried: Int => Int => Int = mk().curried
    val underscore = sum _
    val nullary: () => Unit = unit _
    val nested = three(1)
    val chain = { println("block"); mk() }.add
    println(add(1) + partial(2) + curried(3)(4) + underscore(5, 6) + nested(7)(8) + chain(9))
    nullary()
    println(new Counter(1).getClass)
    println(5.getClass)
    Console.println
