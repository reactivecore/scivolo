enum SUB[-A, +B]:
  case Refl[S]() extends SUB[S, S]

class Pow(self: Int):
  def **(other: Int): Int = math.pow(self, other).toInt

given fromList: [T] => Conversion[List[T], Pow] = ???

def select[T](t1: T, ev: T SUB List[Int]) =
  ev match { case SUB.Refl() => t1 ** 2 }

def ascribed[T](t1: T, ev: T SUB List[Int]) =
  ev match { case SUB.Refl() =>
    val c: Conversion[T, Pow] = fromList
    val d: Pow = t1
    ()
  }
