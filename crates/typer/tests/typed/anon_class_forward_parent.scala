object Test {
  class ZipWithDefault {
    def zero[S]: Test.ZipWith[S]{type T = List[S]} = new ZipWith[S] {
      type T = List[S]
    }
  }
  trait ZipWith[S] {
    type T
  }
}
