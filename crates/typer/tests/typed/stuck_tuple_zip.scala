import scala.Tuple.Zip

type A
type B

def Test =
  summon[Zip[A *: B *: Tuple, B *: A *: Tuple] =:= (A, B) *: (B, A) *: Zip[Tuple, Tuple]]
  summon[Zip[Tuple, Tuple] <:< Tuple]
