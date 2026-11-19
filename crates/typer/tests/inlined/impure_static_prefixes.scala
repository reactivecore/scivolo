object O:
  val k = 10
  inline def f(x: Int): Int = x + k

object P:
  opaque type T = Int
  val base: T = 5
  inline def j(x: Int): Int = x + base
  inline def v(x: Int): T = x * 2

def sideO(): O.type = { println("sideO"); O }
def sideP(): P.type = { println("sideP"); P }

def main(): Unit =
  println(sideO().f(1))
  println(sideP().j(1))
  val t = sideP().v(4)
