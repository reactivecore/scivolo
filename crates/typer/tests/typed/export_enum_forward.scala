import Sig.*

final case class Sig(n: Int) {
  def degree(that: Sig): MatchDegree =
    if n == that.n then FullMatch
    else NoMatch
}

object Sig {
  def first: MatchDegree = NoMatch

  enum MatchDegree {
    case NoMatch
    case FullMatch
  }
  export MatchDegree._
}

object Levels {
  def low: Level = Low

  enum Level {
    case Low, High
  }
  export Level.Low
}
