//> using scala 3.9.0
// Case classes with Unit fields: their hashCode mixes them, equals compares them first.

case class One(u: Unit)
case class Two(s: String, u: Unit)

object unitcases {
  def main(args: Array[String]): Unit = {
    val a = One(())
    val b = Two("x", ())
    println(a == One(()))
    println(b == Two("x", ()))
    println(a.hashCode == One(()).hashCode)
    println(b.hashCode == Two("x", ()).hashCode)
  }
}
