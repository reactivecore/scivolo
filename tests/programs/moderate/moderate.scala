//> using scala 3.9.0

enum Dept:
  case Engineering, Sales, Support

case class Employee(name: String, age: Int, dept: Dept, salary: Double)

sealed trait Shape
case class Circle(r: Double) extends Shape
case class Rect(w: Double, h: Double) extends Shape
case class Tri(a: Double, b: Double, c: Double) extends Shape

def area(s: Shape): Double = s match
  case Circle(r)     => math.Pi * r * r
  case Rect(w, h)    => w * h
  case Tri(a, b, c)  =>
    val p = (a + b + c) / 2
    math.sqrt(p * (p - a) * (p - b) * (p - c))

def describe(x: Any): String = x match
  case i: Int if i < 0      => s"negative int $i"
  case i: Int               => s"int $i"
  case s: String            => s"string of length ${s.length}"
  case (a, b)               => s"pair($a, $b)"
  case xs: List[?]          => s"list of ${xs.size}"
  case _                    => "unknown"

@main def moderate(): Unit =
  val staff = List(
    Employee("Alice", 34, Dept.Engineering, 120000),
    Employee("Bob", 45, Dept.Sales, 90000),
    Employee("Carol", 29, Dept.Engineering, 110000),
    Employee("Dave", 52, Dept.Support, 70000),
    Employee("Eve", 38, Dept.Sales, 95000),
    Employee("Frank", 41, Dept.Support, 72000)
  )
  val byDept = staff.groupBy(_.dept)
  for (d, es) <- byDept.toList.sortBy(_._1.ordinal) do
    val avg = es.map(_.salary).sum / es.size
    println(f"$d%-12s n=${es.size} avg=$avg%.2f names=${es.map(_.name).mkString(",")}")
  val seniors = staff.filter(_.age > 40).map(e => e.copy(salary = e.salary * 1.1))
  println(s"Seniors: ${seniors.map(e => s"${e.name}:${e.salary.toInt}").mkString(", ")}")
  val shapes = List(Circle(1.0), Rect(2, 3), Tri(3, 4, 5), Circle(2.5))
  val total = shapes.map(area).sum
  println(f"Total area: $total%.3f; largest: ${shapes.maxBy(area)}")
  List(-3, 7, "hello", (1, "x"), List(1, 2, 3), 3.14).map(describe).foreach(println)
  val words = "the quick brown fox jumps over the lazy dog the end".split(" ").toList
  val freq = words.groupMapReduce(identity)(_ => 1)(_ + _).toSeq.sortBy((w, c) => (-c, w))
  println(freq.take(3).map((w, c) => s"$w=$c").mkString("top: ", ", ", ""))
  val oldest = staff.foldLeft(Option.empty[Employee]) {
    case (None, e)                      => Some(e)
    case (Some(o), e) if e.age > o.age => Some(e)
    case (acc, _)                       => acc
  }
  println(s"Oldest: ${oldest.map(_.name).getOrElse("?")}")
