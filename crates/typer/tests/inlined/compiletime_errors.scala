import scala.compiletime.*

object Test {
  inline def fail(inline b: Boolean): Int = inline if b then error("failed!") else 1
  inline def twice(inline b: Boolean): Int = fail(b) * 2
  transparent inline def checked(inline n: Int): Int =
    inline if n < 0 then error("negative: " + codeOf(n)) else n

  def main(args: Array[String]): Unit = {
    println(fail(false))
    println(twice(true))
    println(checked(-1))
    error("direct")
  }
}
