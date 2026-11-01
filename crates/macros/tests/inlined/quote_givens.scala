//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.Searches

object QuoteGivens:
  val inBlock = Searches.inBlock
  val inObject = Searches.inObject
  val usingClass = Searches.usingClass

object Proxies:
  def names: List[String] =
    class B
    abstract class B2
    List(Searches.companionName[B], Searches.companionName[B2])
