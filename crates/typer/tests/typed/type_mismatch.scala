class Animal
class Dog extends Animal
class Box(val v: Int)

object Util {
  def twice(i: Int): Int = i * 2
}

class Holder {
  val n = 1
  def text: String = n
  def me: Int = this
}

object TypeMismatch {
  val count = 1
  val a: Int = "a"
  def b: String = 1L
  def c(flag: Boolean): Int = if (flag) 1 else "no"
  def d(i: Int): String = i match {
    case 1 => "one"
    case _ => 'x'
  }
  def e(i: Int): Int = {
    if (i > 0) return true
    i
  }
  def f(i: Int): String = i
  def g: String = {
    val local = 2
    local
  }
  val h: String = count
  def k(o: Option[String]): Option[Int] = o
  def l(xs: List[Int]): String = xs.head
  def m(x: Int): Int = x
  val n = m("n")
  val o = Util.twice(true)
  val p = new Box("p")
  def q(a: Animal): Dog = a
  val r: Dog = new Animal
  val s: Int => Int = x => "s"
  def u(): Unit = {
    var w = 1
    w = "w"
  }
  val v = ("v": Int)
  def x: Int = try "x" catch { case _: Exception => 0 }
  val y: Int = TypeMismatch
}
