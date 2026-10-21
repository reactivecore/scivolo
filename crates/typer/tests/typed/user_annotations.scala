import scala.annotation.{StaticAnnotation, nowarn, varargs}

class command(val name: String = null, val doc: String = null) extends StaticAnnotation
class opt(name: String = null, short: Char = 0, doc: String = null, hidden: Boolean = false)
    extends StaticAnnotation
final class marker extends StaticAnnotation

@marker class Tool:
  @command def run(
      @opt(short = 'f', doc = "file") file: String,
      @opt(name = "my-num", doc = "count") num: Int = 2,
      @opt(doc = "flag", hidden = true) flag: Boolean
  ): String = file * num + flag

  @command("other", doc = "xy") @nowarn("msg=unused") def other(): Int = 1

  @varargs def all(xs: Int*): Int = xs.sum
