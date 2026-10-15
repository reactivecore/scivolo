object O:
  opaque type T = Int
  def t(n: Int): T = n
  extension (x: T) def v: Int = x
  def twice(x: T): T = x + x
  def lifted(x: Int): T = O.t(x)

object P:
  opaque type Pos <: Int = Int
  def pos(n: Int): Pos = n

object Test:
  def main(args: Array[String]): Unit =
    println(O.twice(O.t(2)).v)
    println(P.pos(3) + 1)
