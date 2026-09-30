//> using options -source:3.7 -preview
object O:
  def f(using Quotes) = '[ A | (B, C) ]
  def g = for x <- xs; y = x yield y
