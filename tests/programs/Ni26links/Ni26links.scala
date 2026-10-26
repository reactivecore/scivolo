import scala.scalanative.unsafe.*

@link("m")
@link("c")
@extern object cmath {
  def abs(x: CInt): CInt = extern
}

object Ni26links {
  def main(args: Array[String]): Unit = println(cmath.abs(-5))
}
