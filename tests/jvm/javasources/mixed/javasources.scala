package mixed

import java.util.Arrays

@main def javasources(): Unit =
  val c = new Circle(2.0)
  val built = new Circle.Builder().radius(0.5).build()
  println(c.describe(Point(1, 2)))
  println(Shape.unit().describe(c.center()))
  println(Shape.largest(Arrays.asList(c, built, Square(3.0))).name())
  println(s"${Shape.KIND} ${Shape.SIDES_UNKNOWN} ${Circle.FIRST} ${Circle.LAST} ${Circle.BIG}")
  println(s"${Circle.LETTER} ${Circle.GREETING} ${Circle.created()}")
  println(c.max("pear", "apple"))
  println(Circle.join("-", "a", "b", "c"))
  println(Circle.mapAll(Arrays.asList("x", "y"), java.util.function.Function.identity[String]()))
  println(Circle.Color.values().toList.map(_.next()))
  println(Point(3, 1).color.compareTo(Circle.Color.BLUE))
  println(Circle.Color.valueOf("GREEN"))
  println(c.compareTo(built))
  println(s"${Circle.NEGATED} ${Circle.SIZES} ${Circle.NONE} ${Circle.first[Integer, Integer](1, 2)}")
  println(classOf[Point].getAnnotation(classOf[Tag]).value())
  val p = Pair.of("p")
  val counts: List[Int] = p.counts().toList
  println(s"${p.name()} $counts ${p.total()} ${p.compareTo(Pair("q", 5))} ${Pair.made()}")
  println(p.toString().replaceAll("@[0-9a-f]+", ""))
  print(Pair.BLOCK.replace(' ', '_'))
  val animals: List[Animal] = List(Animal.Cat(), Dog())
  println(animals.map {
    case c: Animal.Cat => "cat " + c.sound()
    case a => "other " + a.sound()
  })
  val t = Tree[String]()
  val n = t.root("r")
  println(s"${n.value} ${t.leaf("l").value} ${n.tree() eq t} ${t.sizes(java.util.List.of("a"))(0)(0)}")
