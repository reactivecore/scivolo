//> using scala 3.9.0
// Synthesized mirrors: products, singletons and sums, by companions or anonymous classes.

import scala.deriving.Mirror
import scala.compiletime.{constValue, constValueTuple}

case class P(id: Int, name: String)
case class Box[T](value: T)
case object O

sealed trait Shape
case class Circle(r: Double) extends Shape
case class Square(side: Double) extends Shape
case object Dot extends Shape

sealed abstract class Animal
object Animal {
  case class Dog(name: String) extends Animal
  case object Cat extends Animal
}

enum Color { case Red, Green, Blue }
enum Opt {
  case Sm(t: Int)
  case Nn
}
sealed trait Tree[+T]
case class Leaf[T](t: T) extends Tree[T]
case object Empty extends Tree[Nothing]

object mirrors {
  inline def label[A](using m: Mirror.Of[A]): String = constValue[m.MirroredLabel]

  inline def labels[A](using m: Mirror.Of[A]): Tuple = constValueTuple[m.MirroredElemLabels]

  def main(args: Array[String]): Unit = {
    val mp = summon[Mirror.ProductOf[P]]
    println(mp.fromProduct((1, "a")))
    println(label[P])
    println(labels[P])
    val mb = summon[Mirror.Of[Box[Int]]]
    println(mb.fromProduct(Tuple1(3)))
    val mo = summon[Mirror.Of[O.type]]
    println(mo.fromProduct(EmptyTuple))
    val ms = summon[Mirror.SumOf[Shape]]
    println(ms.ordinal(Square(1.0)))
    println(ms.ordinal(Dot))
    println(label[Shape])
    println(labels[Shape])
    val ma = summon[Mirror.SumOf[Animal]]
    println(ma.ordinal(Animal.Dog("rex")))
    val mc = summon[Mirror.Of[Color]]
    println(mc.ordinal(Color.Blue))
    println(labels[Color])
    val mr = summon[Mirror.ProductOf[Color.Red.type]]
    println(mr.fromProduct(EmptyTuple))
    val mopt = summon[Mirror.SumOf[Opt]]
    println(mopt.ordinal(Opt.Nn))
    val msm = summon[Mirror.ProductOf[Opt.Sm]]
    println(msm.fromProduct(Tuple1(4)))
    val mt = summon[Mirror.SumOf[Tree[Int]]]
    println(mt.ordinal(Leaf(1)))
    println(labels[Tree[Int]])
    val ml = summon[Mirror.ProductOf[Leaf[String]]]
    println(ml.fromProduct(Tuple1("x")))
  }
}
