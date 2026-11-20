class Memoized extends mixins.Memo

object MemoMain {
  def main(args: Array[String]): Unit = println(new Memoized().twice)
}
