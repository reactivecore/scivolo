//> using options -Yexplicit-nulls

class Node(val k: Int)

class Quiet(message: String | Null) extends Throwable(message, null, false, false)

object One { def unapply(a: Any): Option[Int] = Some(1) }

object Explicitnulls {
  def first[A](x: A): Int = x match {
    case One(b) => b
    case _ => 0
  }

  def main(args: Array[String]): Unit = {
    println(first("a") + first(null))
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
