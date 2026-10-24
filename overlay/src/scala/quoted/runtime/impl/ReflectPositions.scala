// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); changed:
//   compiler/src/scala/quoted/runtime/impl/QuotesImpl.scala
package scala.quoted.runtime.impl

/** The positions and source files of `quotes.reflect`. */
trait ReflectPositionSupport { this: QuotesImpl =>

trait ReflectPositions extends reflectModule { self: reflect.type =>

  object Position extends PositionModule {
    def ofMacroExpansion: Position = ReflectOps.position("expansionPos").asInstanceOf[Position]
    def apply(sourceFile: SourceFile, start: Int, end: Int): Position =
      new PositionImpl(sourceFile.asInstanceOf[SourceFileImpl].file, start, end).asInstanceOf[Position]
  }

  object SourceFile extends SourceFileModule {
    def current: SourceFile = Position.ofMacroExpansion.sourceFile
  }

  given PositionMethods: PositionMethods = methods("PositionMethodsImpl")
  given SourceFileMethods: SourceFileMethods = methods("SourceFileMethodsImpl")

  abstract class PositionMethodsImpl extends PositionMethods {
    extension (self: Position) {
      def start: Int = self.asInstanceOf[PositionImpl].start
      def end: Int = self.asInstanceOf[PositionImpl].end
      def sourceFile: SourceFile = new SourceFileImpl(self.asInstanceOf[PositionImpl].file).asInstanceOf[SourceFile]
      def startLine: Int = self.asInstanceOf[PositionImpl].at(0)
      def startColumn: Int = self.asInstanceOf[PositionImpl].at(1)
      def endLine: Int = self.asInstanceOf[PositionImpl].at(2)
      def endColumn: Int = self.asInstanceOf[PositionImpl].at(3)
      def sourceCode: Option[String] = Option(self.asInstanceOf[PositionImpl].code)
    }
  }

  abstract class SourceFileMethodsImpl extends SourceFileMethods {
    extension (self: SourceFile) {
      def path: String = ReflectOps.text("sourcePath", self.asInstanceOf[SourceFileImpl].file)
      def jpath: java.nio.file.Path = java.nio.file.Paths.get(self.path)
      def getJPath: Option[java.nio.file.Path] = Some(self.jpath)
      def name: String = self.path.substring(self.path.lastIndexOf('/') + 1)
      def content: Option[String] = Some(ReflectOps.text("sourceContent", self.asInstanceOf[SourceFileImpl].file))
    }
  }

  private def methods[T](name: String): T =
    QuoteHost.allocate("scala.quoted.runtime.impl.ReflectPositionSupport$ReflectPositions$" + name).asInstanceOf[T]
}
}
