//> using scala 3.9.0
class Doubled extends java.util.AbstractList[Integer]:
  def get(i: Int): Integer = i * 2
  def size(): Int = 3

@main def jdkabstractlist(): Unit =
  val l = new Doubled
  println(l.contains("x"))
  println(l.contains(4))
  println(l.indexOf(2))
  val jl: java.util.List[Integer] = l
  println(jl.lastIndexOf(4))
  println(jl.subList(1, 3).size())
