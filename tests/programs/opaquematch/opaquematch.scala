//> using scala 3.9.0

object P {
  opaque type St = Int
  object St { final val A: St = 0; final val B: St = 1 }
  var st: St = St.A
  def f() = st match { case St.A => 10; case St.B => 20; case _ => 0 }
}

object opaquematch {
  def main(args: Array[String]): Unit = {
    println(P.f())
    P.st = P.St.B
    println(P.f())
  }
}
