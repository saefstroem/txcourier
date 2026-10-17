package dev.txcourier.core;

import java.math.BigDecimal;
import java.math.BigInteger;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

public final class Tx {
    // Length of the serialized transaction record in bytes.
    public static final int RECORD_LEN = 101;

    // Constants for identifying the type of asset and lengths of various fields.
    private static final int TICKER_LEN = 8;
    private static final int ADDRESS_LEN = 20;
    private static final int QUANTITY_LEN = 16;
    private static final BigInteger COIN_GAS = BigInteger.valueOf(21_000);
    private static final BigInteger TOKEN_GAS = BigInteger.valueOf(100_000);
    private static final int COIN_DECIMALS = 18;
    private static final int MOST_DECIMALS = 18;
    private static final int GWEI_DECIMALS = 9;

    private Tx() {
    }

    // Builds a serialized transaction record from the given parameters.
    public static byte[] build(Chain chain, String symbol, String to, String amount, String nonce, String tip, String maxFee,
            String gas) {
        byte[] recipient = Text.address(to);
        // Ensure the recipient address is not the zero address.
        if (Arrays.equals(recipient, new byte[ADDRESS_LEN])) {
            throw new IllegalArgumentException("zero address would burn what is sent");
        }

        // Determine if the asset is a native coin or a token.
        boolean coin = symbol.equals(chain.symbol());

        // Parse the input fields into BigInteger values.
        BigDecimal exact = new BigDecimal(parsed("amount", amount, MOST_DECIMALS), MOST_DECIMALS).stripTrailingZeros();
        int places = Math.max(exact.scale(), 0);
        BigInteger digits = exact.setScale(places).unscaledValue();
        // Parse nonce
        BigInteger count = parsed("nonce", nonce, 0);
        // Parse the priority fee, max fee, and gas limit.
        BigInteger priority = parsed("priority fee", tip, GWEI_DECIMALS);
        BigInteger most = parsed("max fee", maxFee, GWEI_DECIMALS);
        // Determine the gas limit, using defaults if not specified.
        BigInteger limit = gas.isBlank() ? (coin ? COIN_GAS : TOKEN_GAS) : parsed("gas limit", gas, 0);
        
        // sanity checks
        if (digits.signum() == 0) {
            throw new IllegalArgumentException("amount must be more than zero");
        }
        if (most.signum() == 0 || most.compareTo(priority) < 0) {
            throw new IllegalArgumentException("max fee must be more than zero and at least the priority fee");
        }
        if (limit.compareTo(COIN_GAS) < 0) {
            throw new IllegalArgumentException("gas limit must be at least 21000");
        }
        // Return the serialized transaction record.
        return ByteBuffer.allocate(RECORD_LEN)
                .put(fixed("chain id", BigInteger.valueOf(chain.id()), Long.BYTES))
                .put(fixed("nonce", count, Long.BYTES))
                .put(fixed("priority fee", priority, QUANTITY_LEN))
                .put(fixed("max fee", most, QUANTITY_LEN))
                .put(fixed("gas limit", limit, Long.BYTES))
                .put(Arrays.copyOf(symbol.getBytes(StandardCharsets.US_ASCII), TICKER_LEN))
                .put(recipient)
                .put(fixed("amount", digits, QUANTITY_LEN))
                .put((byte) places)
                .array();
    }

    // Describes a serialized transaction record in a human-readable format.
    public static List<String> describe(byte[] record) {
        // Wrap the serialized transaction record in a ByteBuffer for easy field extraction.
        ByteBuffer fields = ByteBuffer.wrap(record);
        // Extract the chain ID, nonce, priority fee, max fee, and gas limit from the record.
        BigInteger chainId = number(fields, Long.BYTES);
        BigInteger nonce = number(fields, Long.BYTES);
        BigInteger tip = number(fields, QUANTITY_LEN);
        BigInteger maxFee = number(fields, QUANTITY_LEN);
        BigInteger gas = number(fields, Long.BYTES);
        String symbol = new String(bytes(fields, TICKER_LEN), StandardCharsets.US_ASCII).replace("\0", "");
        // Extract the recipient address, amount, and contract address (if applicable).
        byte[] recipient = bytes(fields, ADDRESS_LEN);
        BigInteger digits = number(fields, QUANTITY_LEN);
        int places = fields.get();
        Chain chain = Chain.find(chainId);
        String coin = chain == null ? "native units" : chain.symbol();
        // Determine if the asset is a token or a native coin.
        boolean token = !symbol.equals(coin);
        List<String> lines = new ArrayList<>();
        lines.add((token ? "send     " : "send native    ") + Text.amount(digits, places) + " " + symbol);
        lines.add("to       " + Text.hex(recipient));
        lines.add("chain    " + Chain.describe(chainId));
        lines.add("nonce    " + nonce);
        lines.add("gas      limit " + gas + ", max fee " + Text.amount(maxFee, GWEI_DECIMALS)
                + " gwei, priority fee " + Text.amount(tip, GWEI_DECIMALS) + " gwei");
        lines.add("fees     at most " + Text.amount(maxFee.multiply(gas), COIN_DECIMALS) + " " + coin);
        return lines;
    }

    // Parses a human-readable amount into its base unit representation.
    private static BigInteger parsed(String name, String typed, int decimals) {
        try {
            return Text.amount(typed, decimals);
        } catch (IllegalArgumentException unusable) {
            throw new IllegalArgumentException(name + ": " + unusable.getMessage());
        }
    }

    // Converts a BigInteger into a fixed-width byte array, throwing if it doesn't fit.
    private static byte[] fixed(String name, BigInteger value, int width) {
        if (value.bitLength() > Byte.SIZE * width) {
            throw new IllegalArgumentException(name + " is too large for the courier");
        }
        byte[] minimal = value.toByteArray();
        int copied = Math.min(minimal.length, width);
        byte[] fixed = new byte[width];
        System.arraycopy(minimal, minimal.length - copied, fixed, width - copied, copied);
        return fixed;
    }

    // Reads a fixed-width BigInteger from the ByteBuffer.
    private static BigInteger number(ByteBuffer fields, int width) {
        return new BigInteger(1, bytes(fields, width));
    }

    // Reads a fixed-width byte array from the ByteBuffer.
    private static byte[] bytes(ByteBuffer fields, int width) {
        byte[] bytes = new byte[width];
        fields.get(bytes);
        return bytes;
    }
}
