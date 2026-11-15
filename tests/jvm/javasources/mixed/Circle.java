package mixed;

import java.util.ArrayList;
import java.util.function.Function;

@SuppressWarnings("unused")
public class Circle implements Shape {
    public static final int FIRST = 0, LAST = 0x7f;
    public static final long BIG = -9_000_000_000L;
    public static final char LETTER = 'c';
    public static final String GREETING = "hi\tthere";
    private static int created = FIRST;
    protected final double radius;

    static {
        created = 0;
    }

    public Circle(double radius) {
        this.radius = radius;
        created++;
    }

    public double area() { return Math.PI * radius * radius; }

    public String name() { return "circle"; }

    public static int created() { return created; }

    public <T extends Comparable<T>> T max(T a, T b) { return a.compareTo(b) >= 0 ? a : b; }

    public static String join(String sep, String... parts) { return String.join(sep, parts); }

    public static <A, B> ArrayList<B> mapAll(Iterable<A> in, Function<? super A, ? extends B> f) {
        ArrayList<B> out = new ArrayList<>();
        for (A a : in) out.add(f.apply(a));
        return out;
    }

    public Point center() { return new Point(0, 0); }

    public static class Builder {
        private double r = 1;

        public Builder radius(double r) {
            this.r = r;
            return this;
        }

        public Circle build() { return new Circle(r); }
    }

    public enum Color {
        RED, GREEN {
            @Override
            public String toString() { return "green!"; }
        }, BLUE;

        public Color next() { return values()[(ordinal() + 1) % values().length]; }
    }
}
