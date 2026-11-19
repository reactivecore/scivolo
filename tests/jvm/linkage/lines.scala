package lines

object Calls:
  def extension(s: String): Array[String] =
    s.getClass.getName
      .stripSuffix("$")
      .split('.')

  def update(a: Array[Int], i: Int): Unit = i match
    case 0 => ()
    case n =>
      a(n) = a(n) match
        case -1 => i
        case s => s + i

  @(deprecated @scala.annotation.meta.companionMethod)("use something else", "1.0")
  implicit final class Wrapped[A](private val self: A) extends AnyVal:
    def twice: (A, A) = (self, self)

  @(deprecated @scala.annotation.meta.companionMethod)("use something else", "1.0")
  @(deprecated @scala.annotation.meta.companionClass)("use something else", "1.0")
  implicit final class Both[A](private val self: A) extends AnyVal:
    def twice: (A, A) = (self, self)

class Fields(a: Int, b: Int):
  private var current: AnyRef =
    new Object
  def get = { current = null; current }

  def this(a: Int) = this(
    a,
    a
  )