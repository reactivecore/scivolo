sealed trait Animal { def name: String }
case class Dog(name: String) extends Animal
case class Cat(name: String) extends Animal
trait Loud { def volume: Int = 3 }

object Main {
  type Nullable = String | Null
  def a(x: Dog | Cat): String = x.name
  def b(x: Int | String): String = x.toString
  def c(x: Any): Int = x match
    case _: (Int | Long) => 1
    case _: (Dog & Loud) => 2
    case _ => 0
  def d(x: Nullable): Int = if x == null then 0 else 1
  def e(x: Dog & Loud): Int = x.volume + x.name.length
  def g(b: Boolean): Dog | Cat = if b then Dog("d") else Cat("c")
  val i: Int | String = if true then 1 else "a"
  val l: List[Int | String] = List(1, "a")
  def m(x: (Dog | Cat) & Animal): String = x.name
}
