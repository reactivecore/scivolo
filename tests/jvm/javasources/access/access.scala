import scala.annotation.meta.{field as fld, getter as get}

type FieldColumn = Column @fld
type GetterColumn = Column @get

class Table(@FieldColumn val fieldParam: String, @GetterColumn val getterParam: String):
  @FieldColumn val fieldVal: String = ""
  @GetterColumn val getterVal: String = ""

object Access:
  def codes(s: String): String = s.map(_.toInt).mkString(",")

  def annotated(name: String, annotations: Array[java.lang.annotation.Annotation]): String =
    s"$name ${annotations.map(_.annotationType.getName).mkString(",")}"

  def main(args: Array[String]): Unit =
    println(identity(new pk.Sub).hi())
    println(new settings.Settings().setAttempts(1).setAttempts(2).attempts())
    println(codes(valueOf[TextBlocks.separator.type]))
    val c = classOf[Table]
    for f <- c.getDeclaredFields.sortBy(_.getName) do println(annotated(f.getName, f.getAnnotations))
    for m <- c.getDeclaredMethods.sortBy(_.getName) do println(annotated(m.getName, m.getAnnotations))
