import scala.compiletime.testing.typeChecks

inline def twice(inline s: String): String = s + s
inline def checks(inline code: String): Boolean = typeChecks(code)
inline def fails(): Int = compiletime.error("fails")
inline def show(inline b: Boolean): Boolean = b

object TypeChecks:
  val a = show(typeChecks(twice("1")))
  val b = checks(twice("1"))
  val c = show(typeChecks("twice(\"x\").length"))
  val d = checks("1 + 1")
  val e = show(typeChecks("fails()"))
  def f(): Boolean =
    val s = "1"
    show(typeChecks(s))
