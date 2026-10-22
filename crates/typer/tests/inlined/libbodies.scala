//> using options --fresh -classpath crates/typer/tests/inlined/lib/lib.jar
import inl.Lib

object Test {
  def main(args: Array[String]): Unit = {
    val n = args.length
    println(Lib.kind(n))
    println(Lib.orElse(Some(n)))
    println(Lib.safeDiv(n))
    if n > 3 then Lib.fail("many")
    println(Lib.local(n))
    println(Lib.adder(n)(2))
    println(Lib.pair(n))
    println(Lib.viaTransparent(n))
    println(Lib.describe(0))
    println(Lib.describe(12))
    println(Lib.sign(-4))
    println(Lib.typeName[Int])
    println(Lib.firstOr(Some(n)))
    println(Lib.sepOr())
    println(Lib.sepOr(n))
    println(Lib.sepOr(sep = "x"))
    println(Lib.elemCount[Point])
    println(Lib.elemCount[Shape])
  }
}

case class Point(x: Int, y: Int)
sealed trait Shape
object Shape
case class Circle() extends Shape
case class Square() extends Shape
case class Line() extends Shape
