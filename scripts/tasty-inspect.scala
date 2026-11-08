//> using scala 3.9.0
//> using dep org.scala-lang::scala3-tasty-inspector:3.9.0

// Reads TASTy files through dotty's TASTy inspector, the type and symbol of every tree, and
// prints each file's trees with `--show`; each directory argument is read as one group of the
// TASTy files below it, `--each DIR` as the directories in DIR. Fails if any group does not read.

import java.nio.file.{Files, Path}
import scala.jdk.CollectionConverters.*
import scala.quoted.*
import scala.tasty.inspector.*

@main def inspect(args: String*): Unit =
  val show = args.contains("--show")
  val paths = args.filterNot(_ == "--show")
  val inspector = new Inspector:
    def inspect(using Quotes)(tastys: List[Tasty[quotes.type]]): Unit =
      import quotes.reflect.*
      object Reader extends TreeTraverser:
        override def traverseTree(tree: Tree)(owner: Symbol): Unit =
          tree match
            case t: Term => t.tpe.widen
            case t: TypeTree => t.tpe
            case _ =>
          tree.symbol
          super.traverseTree(tree)(owner)
      for tasty <- tastys do
        println(s"-- ${tasty.path}")
        Reader.traverseTree(tasty.ast)(Symbol.spliceOwner)
        if show then println(tasty.ast.show)
  val expanded = paths match
    case Seq("--each", dir) => Files.list(Path.of(dir)).iterator.asScala.map(_.toString).toSeq.sorted
    case _ => paths
  val (dirs, files) = expanded.partition(p => Files.isDirectory(Path.of(p)))
  val groups = (if files.isEmpty then Nil else List(files.toList)) ++ dirs.map { d =>
    Files.walk(Path.of(d)).iterator.asScala.map(_.toString).filter(_.endsWith(".tasty")).toList.sorted
  }
  def reads(group: List[String]) =
    try TastyInspector.inspectTastyFiles(group)(inspector)
    catch case e: Throwable => { println((s"$e in ${group.head}" +: e.getStackTrace.take(12)).mkString("\n  ")); false }
  val failed = groups.filterNot(g => g.isEmpty || reads(g))
  failed.foreach(g => println(s"FAILED: ${g.mkString(" ")}"))
  if failed.nonEmpty then sys.exit(1)
