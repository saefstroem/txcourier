import dev.txcourier.core.Rpc;
import java.io.IOException;
import java.math.BigInteger;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Deque;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

public final class FakeRpc implements Rpc {
    final List<String> calls = new ArrayList<>();
    private final Map<String, Deque<Object>> replies = new HashMap<>();

    FakeRpc node(String url, Object... answers) {
        replies.put(url, new ArrayDeque<>(Arrays.asList(answers)));
        return this;
    }

    @Override
    public BigInteger[] fees(String url) throws IOException {
        return (BigInteger[]) next(url, "fees");
    }

    @Override
    public String send(String url, String signed) throws IOException {
        return (String) next(url, "send");
    }

    private Object next(String url, String method) throws IOException {
        calls.add(method + " " + url);
        Deque<Object> answers = replies.get(url);
        if (answers == null || answers.isEmpty()) {
            throw new IOException("connection refused by " + url);
        }
        Object answer = answers.removeFirst();
        if (answer instanceof IOException failure) {
            throw failure;
        }
        if (answer instanceof Error failure) {
            throw failure;
        }
        return answer;
    }
}
