import scala.language.experimental.modularity

type Elem[X] = X match
  case String => Char
  case Array[t] => t
  case Iterable[t] => t

type Concat[X <: Tuple, Y <: Tuple] <: Tuple = X match
  case EmptyTuple => Y
  case x *: xs => x *: Concat[xs, Y]

type MapF = [X] =>> Map[X, X]
type Poly = [T] => T => List[T]
val single: [T] => T => List[T] = [T] => (x: T) => List(x)

type U = Int | String
type I = Serializable & Product
opaque type Meters = Double
object Meters:
  def apply(d: Double): Meters = d

def dep(x: AnyRef): x.type = x
trait Container:
  type Elem
  def get: Elem
def extract(c: Container): c.Elem = c.get

def wild(xs: List[?]): Int = xs.size
def byName(x: => Int): Int = x
def varargs(xs: Int*): Int = xs.sum
val tup: (Int, String) = (1, "a")
val lit: 42 = 42
class Tracked(tracked val c: Container)
