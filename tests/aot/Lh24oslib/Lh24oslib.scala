//> using dep com.lihaoyi::os-lib::0.11.9-M8

@main def run(): Unit =
  val name = "file.txt"
  val file = os.pwd / "tmp" / name
  println(file.last + " " + file.ext + " " + file.baseName)
  println(file.relativeTo(os.pwd))
  val rel = os.rel / "a" / "b"
  println(rel)
  println(os.sub / "x" / Seq("y", "z"))
  val p = os.root / "usr" / "lib"
  println(p)
  println(p / os.up)
  println((p / rel).segments.toList)
  println(os.Path("/a/b/c").relativeTo(os.Path("/a/d")))
  println(os.root / "a" / os.RelPath("b/c"))
  println(os.Path("x", os.root))
  println(p.startsWith(os.root / "usr") -> (p / rel).startsWith(p / "a" / "c"))
