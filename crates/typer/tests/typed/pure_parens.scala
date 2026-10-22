object T {
  trait X extends Function1[Int, String]
  implicit def f2x(f: Function1[Int, String]): X = ???
  ({case _ if "".isEmpty => ""} : X)
  ({case _ => ""} : X)
  (1 : Int)
  ( (2 : Int))
  3
}
