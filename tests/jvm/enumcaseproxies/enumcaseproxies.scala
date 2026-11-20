package p

enum Kind:
  def message: String = this match
    case Exhaustivity => "e"
    case Other => "o"
  case Other, Exhaustivity

@main def enumcaseproxies = println(Kind.values.map(_.message).mkString(",") + new Exhaustivity(1).x)
