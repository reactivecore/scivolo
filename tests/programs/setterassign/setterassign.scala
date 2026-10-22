//> using scala 3.9.0
class Counter {
  private var n = 0
  def count: Int = n
  def count_=(x: Int): Unit = n = x
}

class Ctx(val name: String)

class Cell(private var make: Ctx ?=> String) {
  def reset(m: Ctx ?=> String): Unit = this.make = m
  def show(using Ctx): String = make
}

object setterassign {
  def main(args: Array[String]): Unit = {
    val c = new Counter
    c.count = 5
    c.count += 10
    println(c.count)
    given Ctx = new Ctx("ctx")
    val cell = new Cell("a:" + summon[Ctx].name)
    println(cell.show)
    cell.reset("b:" + summon[Ctx].name)
    println(cell.show)
  }
}
