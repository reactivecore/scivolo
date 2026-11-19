package phantoms

enum Color:
  case Red, Green

class Prim:
  lazy val nul: Null = null
  lazy val nothing: Nothing = throw new IllegalStateException("no value")

object Main:
  def main(args: Array[String]): Unit =
    println(Color.valueOf("Red"))
    println(Color.values.toList)
    val p = new Prim
    println(p.nul)
    try p.nothing
    catch case e: IllegalStateException => println(e.getMessage)
