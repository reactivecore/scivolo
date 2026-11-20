import arrays.*

object Arrays {
  def main(args: Array[String]): Unit = {
    val lib = new Lib
    println(lib.first(Array("x", "y")))
    println(lib.first(Array(7, 8)))
    println(lib.refs(Array("a", "b", "c")))
    println(lib.ints(Array(1, 2)))
  }
}
