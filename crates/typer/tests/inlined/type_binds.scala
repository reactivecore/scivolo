import scala.compiletime.*
import scala.compiletime.ops.int.S

case class Desc[T](name: String)
object Desc {
  given Desc[String] = Desc("String")
  given Desc[Int] = Desc("Int")
}

object Test {
  inline def depth[N]: Int =
    inline constValueOpt[N] match {
      case Some(_: S[n]) => 1 + depth[n]
      case Some(0) => 0
      case None => -1
    }

  inline def params[R](f: (?, ?) => R): (Desc[?], Desc[?]) =
    inline f match {
      case _: ((a, b) => R) => (summonInline[Desc[a]], summonInline[Desc[b]])
    }

  def two(s: String, i: Int): Unit = ()

  def main(args: Array[String]): Unit = {
    println(depth[2])
    println(params(two))
  }
}
