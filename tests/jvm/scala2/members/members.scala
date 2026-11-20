import members.*

object Members {
  def main(args: Array[String]): Unit = {
    println(O.lz)
    println(O.lz)
    println(O.c)
    println(O.arr(Array("a", "b")))
    println(O.prim(Array(1)))
    O.unit()
    println(O.curried(1)(2))
    println(O.withOrdering(3))
    O.v = "w"
    println(O.v)
    val g = new Gen("s")
    println(g.t)
    println(g.nul)
    try g.nothing
    catch { case e: IllegalStateException => println(e.getMessage) }
  }
}
