public class Test {
  public static void main(String[] args) {
    Bounds b = new Bounds();
    System.out.println(b.ints(1, 2, 3));
    System.out.println(b.doubles(1.5, 2.5));
    System.out.println(b.comparable("a", "b"));
    System.out.println(b.both("a", "b", "c"));
    System.out.println(b.anyVal(1, 2));
    System.out.println(b.any("a"));
  }
}
