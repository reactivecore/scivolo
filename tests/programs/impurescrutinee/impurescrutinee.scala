object impurescrutinee {
  var n = 0
  def eff(): Int = { n += 1; n }
  class Exactly[T]
  def exact[T]: Exactly[T] = { n += 1; new Exactly[T] }
  inline def exactType[T]: Exactly[T] = exact[T]
  inline def viaInline[T](t: T): T =
    inline exactType[T] match {
      case _: Exactly[Int] => 23
      case _ => t
    }
  inline def direct[T](t: T): T =
    inline exact[T] match {
      case _: Exactly[Int] => 23
      case _ => t
    }
  inline def any(inline x: Int): String = inline x match { case _ => "any" }
  inline def bound(inline x: Int): Int = inline x match { case y: Int => y + 10 }

  def main(args: Array[String]): Unit = {
    println(viaInline(1))
    println(n)
    println(viaInline("a"))
    println(n)
    println(direct(1))
    println(n)
    println(direct("b"))
    println(n)
    println(any(eff()))
    println(n)
    println(bound(eff()))
    println(n)
  }
}
