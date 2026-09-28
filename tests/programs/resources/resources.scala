//> using scala 3.9.0

import java.util.ServiceLoader

trait Greeter:
  def greet: String

class English extends Greeter:
  def greet = "hello"

class German extends Greeter:
  def greet = "hallo"

/** Resource files, `META-INF/services` providers and `Class.forName` from the classpath (ADR
  * 0007); `resources.check` is written by hand, as a Scala Native build embeds no resources by
  * default.
  */
@main def resources(): Unit =
  val in = classOf[Greeter].getResourceAsStream("/greeting.txt")
  println(scala.io.Source.fromInputStream(in).mkString.trim)
  println(classOf[Greeter].getResourceAsStream("/missing.txt") == null)
  val greeters = ServiceLoader.load(classOf[Greeter])
  greeters.forEach(g => println(g.greet))
  println(Class.forName("English") == classOf[English])
  try Class.forName("NoSuchClass")
  catch case e: ClassNotFoundException => println(s"not found: ${e.getMessage}")
