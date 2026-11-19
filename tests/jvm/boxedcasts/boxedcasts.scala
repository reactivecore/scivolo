type Or[+A, +B] = A | B

object Boxedcasts:
  def length(s: String): Int = s.length

  def main(args: Array[String]): Unit =
    val y: Int & String = 3.asInstanceOf[Int & String]
    try println(length(y))
    catch case _: ClassCastException => println("Int & String is no String")
    val x: Or[Int, String] & Or[String, Int] = 3
    val z: Or[Int & String, String & Int] = x
    try println(length(z))
    catch case _: ClassCastException => println("Or[Int & String, String & Int] is no String")
