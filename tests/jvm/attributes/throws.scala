@throws[java.io.IOException]
abstract class Throwing:
  @throws[java.io.IOException]
  lazy val later: Int = 1
  @throws[java.io.IOException]
  val abstractly: List[String]
  @throws[java.io.IOException]
  var field: List[String] = Nil
  @throws[java.io.IOException]
  val plain: Int = 2
  @throws[java.io.IOException]
  def method: Int = 3
  @throws[java.io.IOException]
  final val constant = 4
  @throws[java.io.IOException]
  final val computed: String = "c"

trait ThrowingTrait:
  @throws[java.io.IOException]
  val inTrait: Int = 5

class ThrowingValue(val s: String) extends AnyVal:
  @throws[java.io.IOException]
  def load(n: Int): String = s

object InlineThrows:
  inline def guarded: List[String] =
    class Oops extends Exception
    class Lifted:
      @throws[Oops] def lifted(y: Int): Int = y + 1
    Throws.show(classOf[Lifted])
  def use: List[String] = guarded

@throws[java.io.IOException]
object Throws:
  def show(c: Class[?]): List[String] =
    c.getDeclaredMethods.toList.filterNot(_.getName.contains("lzy")).sortBy(_.getName).map { m =>
      val thrown = m.getExceptionTypes.map(_.getSimpleName).mkString(",")
      val generic = m.getGenericExceptionTypes.map(_.getTypeName).mkString(",")
      s"${m.getName}[$thrown][$generic]"
    }
