object deadoperands:
  def attempt(name: String)(body: => Any): Unit =
    try println(s"$name: ${body}")
    catch case _: NotImplementedError => println(s"$name: not implemented")

  def main(args: Array[String]): Unit =
    attempt("left")((??? : Int) + 1)
    attempt("right")(1 + (??? : Int))
    attempt("long")((??? : Long) + 1)
    attempt("widened")((??? : Int) + 1L)
    attempt("negated")(-(??? : Int))
    attempt("inverted")(~(??? : Long))
    attempt("converted")((??? : Int).toLong)
    attempt("compared")((??? : Int) < 1)
    attempt("equal")((??? : Int) == 1)
    attempt("not")(!(??? : Boolean))
    attempt("condition")(if (??? : Boolean) 1 else 2)
    attempt("concat")((??? : String) + "x")
    attempt("element")((??? : Array[Int])(0))
    attempt("length")((??? : Array[Int]).length)
    attempt("store")((??? : Array[Int])(0) = 1)
    attempt("argument")(math.max((??? : Int), 1))
    attempt("shift")((??? : Int) << 1)
    attempt("double")((??? : Double) * 2)
