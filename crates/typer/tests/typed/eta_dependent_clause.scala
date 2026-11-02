import scala.quoted.*

object Shows {
  private def show(using Quotes)(t: quotes.reflect.Tree): String =
    import quotes.reflect.*
    def all(ts: List[Tree]) = ts.map(show).mkString(", ")
    t match
      case Block(stats, e) => s"Block([${all(stats)}], ${show(e)})"
      case _               => "Tree"

  private def synthetic(name: String): Boolean = name.isEmpty
  def names(s: String) = s.split("\\.").map(n => n.stripPrefix("_$")).filterNot(synthetic)
}
