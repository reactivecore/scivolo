package shapes;

import java.util.function.Function;

public class Box<T> {
    public static final String NAME = "box";
    public static int created = 0;

    private final T value;

    public Box(T value) {
        this.value = value;
        created++;
    }

    public T get() {
        return value;
    }

    public <U> Box<U> map(Function<T, U> f) {
        return new Box<>(f.apply(value));
    }

    public static <T> Box<T> of(T value) {
        return new Box<>(value);
    }

    public static int sum(int... xs) {
        int s = 0;
        for (int x : xs) s += x;
        return s;
    }

    public static class Pair {
        public final int left;
        public int right;

        public Pair(int left, int right) {
            this.left = left;
            this.right = right;
        }

        public int total() {
            return left + right;
        }
    }

    public interface Shape {
        double area();

        static Shape square(double side) {
            return () -> side * side;
        }
    }
}
