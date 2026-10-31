package scala.scalanative.runtime

/** `java.lang.Class.getCanonicalName` from the binary name: a `$` before a member's name becomes
 *  `.`; `null` for a local or anonymous class.
 */
abstract class _Class[A] {
  def getName(): String = ???
  def isArray(): scala.Boolean = ???
  def getComponentType(): _Class[?] = ???

  def getCanonicalName(): String =
    if (isArray()) {
      val component = getComponentType().getCanonicalName()
      if (component == null) null else component + "[]"
    } else {
      val name = getName()
      val module = name.length > 1 && name.endsWith("$")
      val body = if (module) name.substring(0, name.length - 1) else name
      val parts = body.split("\\$", -1)
      var i = 1
      while (i < parts.length && !parts(i).isEmpty && !Character.isDigit(parts(i).charAt(0))) i += 1
      if (i < parts.length) null
      else String.join(".", parts*) + (if (module) "$" else "")
    }
}
