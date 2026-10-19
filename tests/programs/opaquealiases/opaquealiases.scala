//> using scala 3.9.0
case class Person(name: String)

object Names {
  opaque type Name = String
  def apply(s: String): Name = s
  extension (n: Name) def shout: String = n.toUpperCase
}

object Viewers {
  opaque type Viewers = Set[Person]
  def empty: Viewers = Set.empty
  extension (v: Viewers) {
    def add(p: Person): Viewers = v + p
    def names: List[String] = v.toList.map(_.name).sorted
  }
}

object Points {
  opaque type Point = (Int, String)
  def apply(x: Int, label: String): Point = (x, label)
  extension (p: Point) {
    def x: Int = p._1
    def label: String = p._2
  }
}

object opaquealiases {
  def main(args: Array[String]): Unit = {
    val n = Names("ada")
    println(n.shout)
    val v = Viewers.empty.add(Person("bob")).add(Person("al")).add(Person("bob"))
    println(v.names.mkString(","))
    val ps = List(Points(2, "b"), Points(1, "a"))
    println(ps.map(p => p.x + p.label).mkString(" "))
  }
}
