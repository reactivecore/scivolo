//> using scala 3.9.0
@main def jdklist(): Unit =
  val l: java.util.List[String] = new java.util.ArrayList
  l.add("a")
  println(l.size())
  println(l.iterator().next())
