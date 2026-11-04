trait A:
  type B
  def name: Any
  val tag: AnyRef

class Impl(val tag: String) extends A:
  type B = Int
  def name: String = "impl"

object refinedmembers:
  def describe(a: A { def name: String; val tag: String }): String = a.name + "/" + a.tag.length

  def main(args: Array[String]): Unit =
    val i = new Impl("xyz")
    val r: A { def name: String; val tag: String } = i
    println(describe(r))
    val narrowed: Impl { val tag: "xyz" } = i.asInstanceOf[Impl { val tag: "xyz" }]
    val s: "xyz" = narrowed.tag
    println(s)
    val f: Function1[Int, Int] { def apply(x: Int): Int } = x => x + 1
    println(f(41))
