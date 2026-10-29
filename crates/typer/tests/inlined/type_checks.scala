import scala.compiletime.testing.typeChecks

inline def twice(inline s: String): String = s + s
inline def checks(inline code: String): Boolean = typeChecks(code)
inline def fails(): Int = compiletime.error("fails")

object TypeChecks:
  val a = typeChecks(twice("1"))
  val b = checks(twice("1"))
  val c = typeChecks("twice(\"x\").length")
  val d = checks("1 + 1")
  val e = typeChecks("fails()")
