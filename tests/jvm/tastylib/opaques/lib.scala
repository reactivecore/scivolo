package lib

object O:
  opaque type F[X] = List[X]
  opaque type G[X] = List[X]
  opaque type B[X] <: Seq[X] = List[X]
  def mkF: F[Int] = List(1)
  def mkG: G[Int] = List(2)
  def mkB: B[Int] = List(3)

object V:
  opaque type OI[+X] = List[X]
  opaque type Op[-X] = X => Int
  def op: Op[Any] = _ => 4

object Fw:
  export O.{F, G}
  def mkF: F[Int] = O.mkF
  def mkG: G[Int] = O.mkG

object VF:
  export V.Op
  def op: Op[String] = V.op

object P:
  opaque type FB <: [X] =>> Any = [X] =>> X

object PF:
  export P.FB

trait TF:
  def f(x: O.F[Int]): Int

trait TI:
  def f(x: Int): Int

type R[X] = X match
  case List[t] => String
  case Any => Int
