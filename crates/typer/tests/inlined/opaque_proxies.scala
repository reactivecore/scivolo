object refined:
  opaque type Positive = Int
  opaque type Name = String
  inline def Positive(value: Int): Positive = f(value)
  transparent inline def TPositive(value: Int): Positive = f(value)
  inline def name(s: String): Name = s.trim
  def f(x: Positive): Positive = x

object Test:
  def main(args: Array[String]): Unit =
    val x = 9
    val nine = refined.Positive(x)
    val tnine: refined.Positive = refined.TPositive(x)
    val n = refined.name(" a ")
    println(nine)
    println(tnine)
    println(n)
