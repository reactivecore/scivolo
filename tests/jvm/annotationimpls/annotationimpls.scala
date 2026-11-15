class Deprecation extends Deprecated:
  final val annotationType = classOf[Deprecated]
  def forRemoval(): Boolean = true
  def since(): String = "3.9"

class Suppression extends SuppressWarnings:
  final val annotationType = classOf[SuppressWarnings]
  def value = Array("unchecked")

object Annotationimpls:
  def main(args: Array[String]): Unit =
    val d: java.lang.annotation.Annotation = new Deprecation
    println(d.annotationType.getSimpleName)
    println(new Deprecation().since())
    println(new Suppression().value.mkString)
