//> using scala 3.9.0
@main def packageobjects(): Unit =
  try sys.error("boom")
  catch case e: RuntimeException => println(e.getMessage)
  println(sys.props.get("packageobjects.none").isEmpty)
  println(math.max(3, 4))
  println(scala.math.abs(-5))
  println(math.Pi * 2 + scala.math.E)
