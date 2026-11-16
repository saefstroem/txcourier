package dev.txcourier.app;

import dev.txcourier.core.Refused;
import dev.txcourier.core.Rpc;
import dev.txcourier.core.Text;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.math.BigInteger;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.TimeUnit;
import javax.net.ssl.HttpsURLConnection;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;
import org.json.JSONTokener;

/**
 * An implementation of the Rpc interface that communicates with Ethereum nodes over HTTPS.
 */
public final class HttpRpc implements Rpc {
    // Timeout for network requests in milliseconds.
    private static final int TIMEOUT_MILLIS = 5_000;
    // Maximum size of the response body in bytes.
    private static final int MAX_ANSWER = 1 << 20;
    // Executor service for scheduling network request timeouts.
    private static final ScheduledExecutorService WATCHDOG = Executors.newSingleThreadScheduledExecutor();

    @Override
    // Retrieves the current gas fees from the Ethereum node at the specified URL.
    public BigInteger[] fees(String url) throws IOException {
        BigInteger tip = quantity(call(url, "eth_maxPriorityFeePerGas", new JSONArray()));
        Object history = call(url, "eth_feeHistory", new JSONArray().put("0x1").put("latest").put(new JSONArray()));
        JSONArray baseFees = history instanceof JSONObject object ? object.optJSONArray("baseFeePerGas") : null;
        if (baseFees == null || baseFees.length() == 0) {
            throw new IOException("fee history has no base fee");
        }
        // return tuple of (base fee, priority fee)
        return new BigInteger[] {quantity(baseFees.opt(baseFees.length() - 1)), tip};
    }

    @Override
    /**
     * Sends a signed transaction to the Ethereum node at the specified URL.
     */
    public String send(String url, String signed) throws IOException {
        return String.valueOf(call(url, "eth_sendRawTransaction", new JSONArray().put(signed)));
    }

    @Override
    // reads balance from the Ethereum node for the specified address
    public BigInteger balance(String url, String address) throws IOException {
        return quantity(call(url, "eth_getBalance", new JSONArray().put(Text.hex(Text.address(address))).put("latest")));
    }

    @Override
    // reads token balance from the Ethereum node for the specified contract and address
    public BigInteger tokenBalance(String url, String contract, String address) throws IOException {
        // balanceof call with the address
        String data = "0x70a08231" + "0".repeat(24) + Text.hex(Text.address(address)).substring(2);
        try {
            // prepare the transaction object for the eth_call
            JSONObject transaction = new JSONObject().put("to", Text.hex(Text.address(contract))).put("data", data);
            // execute the call and conver result into quantity
            return quantity(call(url, "eth_call", new JSONArray().put(transaction).put("latest")));
        } catch (JSONException unencodable) {
            throw new IOException(unencodable);
        }
    }

    @Override
    public BigInteger nonce(String url, String address) throws IOException {
        return quantity(call(url, "eth_getTransactionCount", new JSONArray().put(Text.hex(Text.address(address))).put("latest")));
    }

    // performs an HTTP GET request to the specified URL and returns the response as an Object
    static Object get(String url) throws IOException {
        return fetch(url, null, true);
    }

    // performs an HTTP POST request to the specified URL with the given text and returns the response as a String
    static String post(String url, String text) throws IOException {
        return String.valueOf(fetch(url, text.getBytes(StandardCharsets.US_ASCII), false));
    }

    // Makes a JSON-RPC call to the Ethereum node at the specified URL with the given method and parameters.
    // Throws an IOException if the call fails or the response is invalid.
    private static Object call(String url, String method, JSONArray params) throws IOException {
        if (!(fetch(url, request(method, params), true) instanceof JSONObject answer)) {
            throw new IOException("node's answer is not a JSON-RPC object");
        }
        // check for JSON-RPC error and presence of result field
        JSONObject error = answer.optJSONObject("error");
        // if there is an error object, throw an exception with its message and code
        if (error != null) {
            throw new Refused("node refused: " + Text.clean(error.optString("message") + " (code " + error.optLong("code") + ")"));
        }
        // if there is no error, ensure that the result field is present
        if (!answer.has("result")) {
            throw new IOException("node's answer has neither a result nor an error");
        }
        return answer.opt("result");
    }

    private static Object fetch(String url, byte[] request, boolean json) throws IOException {
        if (!url.startsWith("https://")) { // only https allowed
            throw new IOException(url + " is not an https url");
        }
        HttpsURLConnection connection = (HttpsURLConnection) URI.create(url).toURL().openConnection();
        // Timeout this future and disconnect the connection if it takes too long
        ScheduledFuture<?> timeUp = WATCHDOG.schedule(connection::disconnect, TIMEOUT_MILLIS, TimeUnit.MILLISECONDS);
        try {
            connection.setInstanceFollowRedirects(false);
            connection.setConnectTimeout(TIMEOUT_MILLIS);
            connection.setReadTimeout(TIMEOUT_MILLIS);
            // set up the connection for sending the request if present
            if (request != null) {
                connection.setRequestProperty("Content-Type", json ? "application/json" : "text/plain");
                connection.setDoOutput(true);
                try (OutputStream out = connection.getOutputStream()) {
                    out.write(request);
                }
            }
            int status = connection.getResponseCode();
            InputStream body = status < 400 ? connection.getInputStream() : connection.getErrorStream();
            // read the response body as a string
            String text = body == null ? "" : read(body);
            // if the status indicates an error and the request was not JSON, 
            // throw an exception with the status and cleaned text
            if (status >= 400 && (request == null || !json)) {
                throw new IOException("HTTP " + status + " from " + url + ": " + Text.clean(text));
            }
            return json ? answer(text, status) : text;
        } finally {
            timeUp.cancel(false);
            connection.disconnect();
        }
    }

    // Constructs a JSON-RPC request with the specified method and parameters.
    private static byte[] request(String method, JSONArray params) throws IOException {
        try {
            return new JSONObject().put("jsonrpc", "2.0").put("id", 1).put("method", method).put("params", params)
                    .toString().getBytes(StandardCharsets.UTF_8);
        } catch (JSONException unencodable) {
            throw new IOException(unencodable);
        }
    }

    // Parses the JSON-RPC response text and returns it as a JSONObject.
    // Throws an IOException if the response is not valid JSON.
    private static Object answer(String text, int status) throws IOException {
        try {
            return new JSONTokener(text).nextValue();
        } catch (JSONException | StackOverflowError garbled) {
            throw new IOException("HTTP " + status + " without a JSON answer");
        }
    }

    // Reads the response body from the input stream and returns it as a string.
    // Throws an IOException if the response is too large or cannot be read.
    private static String read(InputStream body) throws IOException {
        try (body) {
            byte[] text = body.readNBytes(MAX_ANSWER + 1);
            if (text.length > MAX_ANSWER) {
                throw new IOException("node's answer is longer than " + MAX_ANSWER + " bytes");
            }
            return new String(text, StandardCharsets.UTF_8);
        }
    }

    // Converts a JSON-RPC quantity (hex string) to a BigInteger.
    private static BigInteger quantity(Object value) throws IOException {
        String text = String.valueOf(value);
        if (!text.matches("0x[0-9a-fA-F]{1,64}")) {
            throw new IOException("node's number " + Text.clean(text) + " is not hex");
        }
        return new BigInteger(text.substring(2), 16);
    }
}
