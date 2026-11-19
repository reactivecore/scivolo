object DeepBynameImplicits {
  trait Generic[T] { type Repr }
  object Generic {
    type Aux[T, R] = Generic[T] { type Repr = R }
    implicit def genTuple3[T, U, V]: Aux[(T, U, V), (T, (U, (V, Unit)))] = ???
    implicit def genTuple5[T, U, V, W, X]: Aux[(T, U, V, W, X), (T, (U, (V, (W, (X, Unit)))))] = ???
  }

  trait Show[T]
  object Show {
    implicit val showUnit: Show[Unit] = ???
    implicit val showInt: Show[Int] = ???
    implicit def showPair[T, U](implicit st: Show[T], su: Show[U]): Show[(T, U)] = ???
    implicit def showGen[T, R](implicit gen: Generic.Aux[T, R], sr: => Show[R]): Show[T] = ???
  }

  type I5 = (Int, Int, Int, Int, Int)

  def test = implicitly[Show[(I5, (I5, I5, I5), Int)]]
}
