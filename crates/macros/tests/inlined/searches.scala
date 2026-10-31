//> using options --fresh -classpath tests/macros/lib/macros.jar
import macrolib.{Searches, Shown}

object Searching:
  val ignored = Searches.foundIgnoringAuto[Shown[Int]]
  val positive: PartialFunction[Int, Int] = { case x if x > 0 => x }
  val picked = Searches.picked(positive)

object Given:
  given mine: Shown[Int] = new Shown[Int] { def shown = "mine" }
  val found = Searches.foundIgnoringAuto[Shown[Int]]
