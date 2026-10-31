import scala.compiletime.codeOf

class Ops(val s: String) {
  inline def twice: String = s + s
}
inline def ops(ctx: => String): Ops = new Ops(ctx)

object Test {
  transparent inline def poly(inline f: [X] => X => X): Int = f[Int](1)
  inline def pick[T](a: T, b: T): T = a
  inline def one: Int = 1
  def dep: (i: Int) => i.type = (i: Int) => i

  def main(args: Array[String]): Unit = {
    println(codeOf(poly([X] => (x: X) => { println(x); x })))
    println(codeOf(Test.pick[Int](one, 2)))
    println(codeOf(one))
    println(dep(3))
    println(ops({ println("ctx"); "a" }).twice)
  }
}
