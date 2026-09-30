package oracle

class Box { type T; type U = Int; val v: AnyRef = "" }
val a: Box = new Box
val b: Box = new Box
object O extends Box

class Outer { class Inner; class Sub extends Inner }
val outer1: Outer = new Outer
val outer2: Outer = new Outer

trait HK { type F[X] <: Seq[X]; type G[X] }
val hk: HK = null

opaque type Pos <: Int = Int

type L1 = [X] =>> List[X]
type L2 = [X] =>> Seq[X]
type Pair[+A] = (A, A)

trait Structural { def f(i: Int): Int; val x: Int }
class Impl extends Structural { def f(i: Int): Int = i; val x: Int = 1 }
type HasX = { val x: Int }
type HasF = { def f(i: Int): Int }
type HasFString = { def f(s: String): Int }
type BoxInt = Box { type T = Int }
type AB = Structural & Box

trait Animal
class Dog extends Animal
class Cov[+A]
class Contra[-A]
class Inv[A]
