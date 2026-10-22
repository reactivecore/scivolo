//> using dep com.lihaoyi::os-lib::0.11.9-M8

@main def run(): Unit =
  val root = os.temp.dir(prefix = "oslib")
  val a = root / "a"
  os.makeDir.all(a / "b")
  os.write(a / "one.txt", "one\n")
  os.write.over(a / "b" / "two.txt", "two")
  os.write.append(a / "one.txt", "more\n")
  println(os.read(a / "one.txt"))
  println(os.read.lines(a / "one.txt").size)
  println(os.list(a).map(_.relativeTo(root).toString).sorted)
  println(os.walk(root).map(_.relativeTo(root).toString).sorted)
  os.copy(a, root / "c")
  println(os.walk(root / "c").filter(os.isFile).map(_.relativeTo(root).toString).sorted)
  os.move(root / "c" / "one.txt", root / "c" / "uno.txt")
  println(os.exists(root / "c" / "one.txt") -> os.exists(root / "c" / "uno.txt"))
  println(os.size(root / "c" / "uno.txt"))
  println(os.isDir(root / "c" / "b"))
  os.remove.all(root)
  println(os.exists(root))
