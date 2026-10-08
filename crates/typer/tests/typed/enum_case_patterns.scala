enum Shape:
  case Circle(r: Double)
  case Rect(w: Double, h: Double)
  case Empty
  def area: Double = this match
    case Circle(r) => 3.0 * r * r
    case Rect(w, h) => w * h
    case Empty => 0.0
