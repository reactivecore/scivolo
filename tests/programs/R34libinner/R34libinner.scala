//> using scala 3.9.0
abstract class R34Enum extends Enumeration {
  def make(): Value = new Val(nextId) {}
  def named(): Value = new Val(nextId, "named")
}

object R34Tab extends R34Enum {
  val a = make()
  val b = named()
}

object R34libinner {
  def main(args: Array[String]): Unit = {
    println(R34Tab.a.id)
    println(R34Tab.b)
    println(R34Tab.values.size)
  }
}
