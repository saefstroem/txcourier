public final class Check {
    private Check() {
    }

    static void that(boolean condition, String what) {
        if (!condition) {
            throw new AssertionError(what);
        }
    }

    static String failure(Attempt action) {
        try {
            action.run();
        } catch (Exception failed) {
            return failed.getMessage();
        }
        throw new AssertionError("accepted what should have failed");
    }
}
