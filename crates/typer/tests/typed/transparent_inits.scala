object Pur {
  transparent inline def ap(inline f: Any => Int): Any = f(O2)
  def main(args: Array[String]): Unit = {
    println(ap(_ => 3))
    println(ap(_ => 4) + 1)
  }
}
object O2 extends T
trait T { println("T init") }
