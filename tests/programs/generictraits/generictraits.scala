//> using scala 3.9.0
trait Holder[A]:
  def get: A
  def describe: String = s"holds $get"

trait Stack[A]:
  var items: List[A] = Nil
  def push(a: A): Unit = items = a :: items
  def top: A = items.head

trait Pair[A, B]:
  def first: A
  def second: B
  def swap: (B, A) = (second, first)

class IntHolder(n: Int) extends Holder[Int]:
  def get: Int = n

class NameHolder extends Holder[String]:
  def get: String = "name"

object Strings extends Stack[String]

class P extends Pair[String, Int]:
  val first = "one"
  def second: Int = 1

@main def generictraits(): Unit =
  println(IntHolder(3).describe)
  println(NameHolder().describe)
  Strings.push("a")
  Strings.push("b")
  println(Strings.top)
  println(Strings.items)
  println(P().swap)
