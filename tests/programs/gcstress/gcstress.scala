//> using scala 3.9.0

import scala.collection.mutable

final case class Node(id: Int, name: String, next: Node)

/** Allocates far more than it keeps; the kept structures must survive every collection intact. */
@main def gcstress(args: String*): Unit =
  val rounds = args.headOption.fold(12)(_.toInt)
  val kept = mutable.ArrayBuffer.empty[Node]
  val table = mutable.HashMap.empty[String, Array[Int]]
  var checksum = 0L
  for round <- 0 until rounds do
    var list: Node = null
    for i <- 0 until 20000 do list = Node(i, s"n$i", list)
    var n = list
    while n != null do
      checksum += n.id + n.name.length
      n = n.next
    val big = new Array[Long](200000)
    big(round) = round.toLong
    checksum += big.sum
    if round % 4 == 0 then kept += list
    table(s"r$round") = Array.fill(100)(round)
    if table.size > 8 then table.remove(s"r${round - 8}")
  val keptOk = kept.forall { head =>
    var n = head
    var expected = 19999
    while n != null && n.id == expected && n.name == s"n$expected" do
      n = n.next
      expected -= 1
    n == null && expected == -1
  }
  val tableOk = table.forall((k, v) => v.forall(_ == k.drop(1).toInt))
  println(s"checksum $checksum")
  println(s"kept ${kept.size} lists intact: $keptOk")
  println(s"table ${table.size} entries intact: $tableOk")
