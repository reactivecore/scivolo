//> using scala 3.9.0

final class Point(val x: Int):
  override def equals(o: Any): Boolean = o match
    case p: Point => p.x == x
    case _        => false
  override def hashCode: Int = x

def same(a: Any, b: Any): Boolean = a == b

@main def boxeq(): Unit =
  val nan: Any = Double.NaN
  val nanF: Any = Float.NaN
  val cases = List[(String, Any, Any)](
    ("int", 1000, 1000),
    ("int ne", 1000, 1001),
    ("long", 5000000000L, 5000000000L),
    ("int long", 7, 7L),
    ("byte short", 3.toByte, 3.toShort),
    ("char", 'x', 'x'),
    ("char int", 'A', 65),
    ("bool", true, true),
    ("bool ne", true, false),
    ("double", 1.5, 1.5),
    ("zeros", 0.0, -0.0),
    ("float zeros", 0.0f, -0.0f),
    ("nan same box", nan, nan),
    ("float nan same box", nanF, nanF),
    ("double float", 0.5, 0.5f),
    ("null null", null, null),
    ("null int", null, 1),
    ("int null", 1, null),
    ("string", "ab", "a" + "b"),
    ("point", Point(3), Point(3)),
    ("point ne", Point(3), Point(4)),
    ("bigint int", BigInt(9), 9)
  )
  for (name, a, b) <- cases do println(s"$name: ${same(a, b)}")
  var hits = 0
  var i = 0
  while i < 200000 do
    if same(i, i % 1000 + (i / 1000) * 1000) then hits += 1
    i += 1
  println(s"loop: $hits")
