package dev.txcourier.core;

import java.math.BigDecimal;
import java.math.BigInteger;
import java.util.HexFormat;

public final class Text {
    private static final int MAX_BITS = 256;
    private static final int MAX_SHOWN = 300;

    // Utility class for text manipulation and conversion related to blockchain data.
    private Text() {
    }

    // Converts a string representation of an amount to a BigInteger, considering the specified number of decimals.
    public static BigInteger amount(String typed, int decimals) {
        String number = typed.strip().replace(',', '.');
        if (!number.matches("[0-9]*\\.?[0-9]*") || number.replace(".", "").isEmpty()) {
            throw new IllegalArgumentException("'" + clean(typed) + "' is not a number");
        }
        BigInteger value;
        try {
            value = new BigDecimal(number).movePointRight(decimals).toBigIntegerExact();
        } catch (ArithmeticException tooPrecise) {
            throw new IllegalArgumentException("'" + clean(typed) + "' has more than " + decimals + " decimal places");
        }
        if (value.bitLength() > MAX_BITS) {
            throw new IllegalArgumentException("'" + clean(typed) + "' is too large");
        }
        return value;
    }

    // Converts a BigInteger amount to its string representation, considering the specified number of decimals.
    public static String amount(BigInteger value, int decimals) {
        return new BigDecimal(value, decimals).stripTrailingZeros().toPlainString();
    }

    // Converts a string representation of an Ethereum address to its byte array form.
    public static byte[] address(String typed) {
        String digits = typed.strip();
        if (!digits.matches("(0x)?[0-9a-fA-F]{40}")) {
            throw new IllegalArgumentException("'" + clean(typed) + "' is not 0x and 40 hex digits");
        }
        return bytes(digits);
    }

    // Converts a hexadecimal string to its corresponding byte array.
    public static byte[] bytes(String hex) {
        String digits = hex.strip();
        return HexFormat.of().parseHex(digits.startsWith("0x") ? digits.substring(2) : digits);
    }

    // Converts a byte array to its hexadecimal string representation, prefixed with "0x".
    public static String hex(byte[] bytes) {
        return "0x" + HexFormat.of().formatHex(bytes);
    }

    // Cleans the input text by removing control characters and certain Unicode formatting characters, truncating if necessary.
    public static String clean(String text) {
        String kept = text.replaceAll("[\\p{Cc}\\u202A-\\u202E\\u2066-\\u2069]", "");
        return kept.length() > MAX_SHOWN ? kept.substring(0, MAX_SHOWN) : kept;
    }

    // Extracts and cleans the reason message from a Throwable, using the class name if the message is null.
    public static String reason(Throwable failure) {
        return clean(failure.getMessage() == null ? failure.getClass().getSimpleName() : failure.getMessage());
    }
}
