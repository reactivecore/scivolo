trait T { def x: Int; val y: Int = 1; def z: String }
class C extends T {
  inline val x = 3
  override inline val y = 4
  inline val z = "zz"
  inline val w = 7
}
abstract class B { val v: Int }
class D extends B { inline val v = 9 }
trait Tr extends T { inline val x = 5; inline val z = "tr" }
class E extends Tr

object Inlinevals {
  def main(args: Array[String]): Unit =
    val t: T = new C
    println((t.x, t.y, t.z))
    val c = new C
    println((c.x, c.y, c.z, c.w))
    val b: B = new D
    println(b.v)
    val e: T = new E
    println((e.x, e.y, e.z))
}
