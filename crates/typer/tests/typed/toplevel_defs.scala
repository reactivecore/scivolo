val c = 1
def twice(x: Int): Int = 2 * x
type Name = String
class Box(val n: Int)

object Main {
  def main(args: Array[String]): Unit = {
    val y = c
    val z: Name = "z"
    println(twice(y) + new Box(c).n + z.length)
  }
}
