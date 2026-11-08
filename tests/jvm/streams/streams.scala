import java.util.stream.{Collectors, IntStream, Stream}

object Streams {
  def main(args: Array[String]): Unit = {
    println(IntStream.rangeClosed(1, 5).map(i => i * i).sum())
    val words = java.util.List.of("b", "a", "c")
    println(words.stream().sorted().collect(Collectors.joining(",")))
    println(words.stream().filter(s => s > "a").count())
    println(Stream.of("g").collect(Collectors.toList()))
    println(IntStream.range(0, 7).filter(i => i % 2 == 0).boxed().collect(Collectors.toList()))
    val list: java.util.AbstractList[String] =
      if (args.length > 0) new java.util.ArrayList[String](words) else new java.util.LinkedList[String](words)
    println(list.size())
  }
}
