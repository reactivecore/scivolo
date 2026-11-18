type Boxed[X] <: List[Any] = X match {
  case Int => List[Int]
  case String => List[String]
}

inline def boxed[T](t: T): Boxed[T] = List(t).asInstanceOf[Boxed[T]]

def first[A](xs: List[A]): A = xs.head

def number = first(boxed(1))
def text = first(boxed("a"))

def kind(x: Any) = "any"
def kind(x: Int) = "int"

inline def insideInline = kind(first(boxed(1)))
def outsideInline = kind(first(boxed(1)))

@main def Test =
  val n: Int = number
  val s: String = text
  println(s"$n $s")
  println(s"$insideInline $outsideInline")
