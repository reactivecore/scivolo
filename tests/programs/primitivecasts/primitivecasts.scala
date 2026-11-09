//> using scala 3.9.0
def attempt(what: String)(body: => Any): Unit =
  try println(s"$what: ${body}")
  catch case e: ClassCastException => println(s"$what: ${e.getClass.getName} ${e.getMessage}")

@main def primitivecasts(): Unit =
  val t = System.nanoTime() != 0
  attempt("Boolean to Int")(t.asInstanceOf[Int])
  attempt("Int to Boolean")(1.asInstanceOf[Boolean])
  attempt("Char to Boolean")('a'.asInstanceOf[Boolean])
  attempt("Unit to Int")(().asInstanceOf[Int])
  attempt("dropped operand")({ println("evaluated"); t }.asInstanceOf[Long])
  attempt("Boolean to Boolean")(t.asInstanceOf[Boolean])
  attempt("Int to Long")(1.asInstanceOf[Long])
  attempt("Char to Int")('a'.asInstanceOf[Int])
