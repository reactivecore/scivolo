package named;

public class Named {
    public final int value;

    public Named(int first, int second) {
        value = first * 10 - second;
    }

    public static int minus(int left, int right) {
        return left - right;
    }
}
