//> using options -Yexplicit-nulls

class Node(val k: Int)

class Quiet(message: String | Null) extends Throwable(message, null, false, false)

object Explicitnulls {
  def main(args: Array[String]): Unit = {
    var table: Array[Node | Null] = new Array[Node | Null](2)
    table(0) = new Node(7)
    table = java.util.Arrays.copyOf(table, 4)
    println(table.length)
    println(table(0).nn.k)
    println(table(3) == null)
    println(new Quiet(null).getMessage() == null)
    println(System.getProperty("scivolo.absent", null) == null)
  }
}
