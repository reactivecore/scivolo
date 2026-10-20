//> using scala 3.9.0
// Hashed long class names, inherited package object members, overridden context function results,
// abstract variables.

package pk {
  trait Greets { def greet(n: Long) = s"long $n"; def greet(n: String) = s"string $n" }
  package object sub extends Greets
}

class AnOuterClassWithAVeryLongNameAnOuterClassWithAVeryLongNameAnOuterClassWithAVeryLongNameAnOuterClassWithAVeryLongName {
  class AnInnerClassWithAVeryLongNameAnInnerClassWithAVeryLongNameAnInnerClassWithAVeryLongNameAnInnerClassWithAVeryLongName {
    def name = "inner"
  }
  object AnInnerObjectWithAVeryLongNameAnInnerObjectWithAVeryLongNameAnInnerObjectWithAVeryLongNameAnInnerObjectWithAVeryLongName {
    def name = "object"
  }
  trait AnInnerTraitWithAVeryLongNameAnInnerTraitWithAVeryLongNameAnInnerTraitWithAVeryLongNameAnInnerTraitWithAVeryLongName {
    def name = "trait"
  }
}

type Scaled[T] = Int ?=> T

class Scorer {
  def score(k: Int): Scaled[Int] = summon[Int] + k
}

class Doubler extends Scorer {
  override def score(k: Int): Scaled[Int] = summon[Int] * k * 2
}

trait Counter { var count: Int }

abstract class Named { var name: String }

class Person extends Named { var name = "ann" }

object lowergaps {
  def main(args: Array[String]): Unit = {
    println(pk.sub.greet(3L))
    println(pk.sub.greet("s"))
    val o = new AnOuterClassWithAVeryLongNameAnOuterClassWithAVeryLongNameAnOuterClassWithAVeryLongNameAnOuterClassWithAVeryLongName
    println(new o.AnInnerClassWithAVeryLongNameAnInnerClassWithAVeryLongNameAnInnerClassWithAVeryLongNameAnInnerClassWithAVeryLongName().name)
    println(o.AnInnerObjectWithAVeryLongNameAnInnerObjectWithAVeryLongNameAnInnerObjectWithAVeryLongNameAnInnerObjectWithAVeryLongName.name)
    println(new o.AnInnerTraitWithAVeryLongNameAnInnerTraitWithAVeryLongNameAnInnerTraitWithAVeryLongNameAnInnerTraitWithAVeryLongName {}.name)
    given Int = 10
    val scorers = List(new Scorer, new Doubler)
    println(scorers.map(_.score(3)))
    val c = new Counter { var count = 1 }
    c.count += 2
    println(c.count)
    val p: Named = new Person
    p.name = p.name + "e"
    println(p.name)
  }
}
