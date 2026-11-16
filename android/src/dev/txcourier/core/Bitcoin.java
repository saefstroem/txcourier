package dev.txcourier.core;

import java.math.BigInteger;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Locale;

public final class Bitcoin {
    static final String ADDRESS = "bc1q[02-9ac-hj-np-z]{38}"; //regex for btc address, see github.com/bitcoin/bips/blob/b34d9f751791/bip-0173.mediawiki?plain=1#L64 github.com/bitcoin/bips/blob/b34d9f751791/bip-0173.mediawiki?plain=1#L89
    private static final int ADDRESS_LEN = 42; // length of a bc1q address, see github.com/bitcoin/bips/blob/b34d9f751791/bip-0173.mediawiki?plain=1#L64
    private static final int TXID_LEN = 32; // length of a transaction ID, see developer.bitcoin.org/reference/transactions.html#outpoint
    private static final int AMOUNT_AT = ADDRESS_LEN; // offset of the amount field in the transaction record, see crates/protocol/src/bitcoin/draft.rs
    private static final long WHOLE_COIN = 0xFFFF_FFFF_FFFF_FFFFL;
    private static final int RATE_DECIMALS = 3;
    private static final int RECIPIENT_AT = AMOUNT_AT + 8 + 8 + TXID_LEN + 4 + 8; // offset of the recipient field in the transaction record, see crates/protocol/src/bitcoin/draft.rs
    private static final int DECIMALS = 8; // number of decimal places in a Bitcoin amount, see github.com/bitcoin/bitcoin/blob/v32.1/src/consensus/amount.h#L15
    public static final long MAX_MONEY = 2_100_000_000_000_000L; // maximum amount of satoshis in existence, see github.com/bitcoin/bitcoin/blob/v32.1/src/consensus/amount.h#L26

    private Bitcoin() {
    }

    // Serialize a txcourier bitcoin transmission
    public static byte[] build(String from, UnspentOutput coin, String to, String amount, String rate) {
        // Normalize and validate the recipient address
        String recipient = to.strip().toLowerCase(Locale.ROOT);
        if (!recipient.matches(ADDRESS)) {
            throw new IllegalArgumentException("'" + Text.clean(to) + "' is not a 42-character bc1q address");
        }
        // Parse and validate the fee rate
        long perThousand = sats("fee rate", Tx.parsed("fee rate", rate, RATE_DECIMALS));
        if (perThousand == 0) {
            throw new IllegalArgumentException("fee rate must be more than zero");
        }
        // Determine if this is a sweep transaction (sending the entire coin)
        boolean sweep = amount.isBlank();

        // Calculate the send amount based on whether this is a sweep transaction
        long send = sweep ? WHOLE_COIN : sats("amount", Tx.parsed("amount", amount, DECIMALS));
        if (send == 0) { // If the send amount is zero, it is invalid
            throw new IllegalArgumentException("amount must be more than zero");
        }
        // Return the serialized transaction as a byte array
        return ByteBuffer.allocate(RECIPIENT_AT + recipient.length()) // allocate full size courier RECORD_LEN
                .put(from.getBytes(StandardCharsets.US_ASCII)) // put the sender address
                .putLong(send) // put the send amount
                .putLong(perThousand) // put the fee rate
                .put(Text.bytes(coin.txid())) // put the transaction ID
                .putInt(coin.vout()) // put the output index
                .putLong(coin.sats()) // put the coin amount
                .put(recipient.getBytes(StandardCharsets.US_ASCII)) // put the recipient address
                .array();
    }

    // Describe a serialized transaction record as a list of human-readable strings
    public static List<String> describe(byte[] record) {
        // Create a bytebuffer to read the transaction fields from the serialized record
        // step by step
        ByteBuffer fields = ByteBuffer.wrap(record, AMOUNT_AT, RECIPIENT_AT - AMOUNT_AT);
        long send = fields.getLong(); // read the send amount
        long perThousand = fields.getLong(); // read the fee rate
        byte[] txid = new byte[TXID_LEN];
        fields.get(txid); // read the transaction ID
        int vout = fields.getInt(); // read the output index
        long coin = fields.getLong(); // read the coin amount
        String from = new String(record, 0, ADDRESS_LEN, StandardCharsets.US_ASCII); // read the sender address
        String to = new String(record, RECIPIENT_AT, record.length - RECIPIENT_AT, StandardCharsets.US_ASCII); // read the recipient address
        boolean sweep = send == WHOLE_COIN;
        
        // Describe the transaction as a list of human-readable strings
        return List.of(
                "send     " + (sweep ? "whole coin, less the fee" : bitcoins(send) + " BTC"),
                "to       " + to,
                "change   " + (sweep ? "none" : "any rest back to " + from),
                "fee      " + Text.amount(BigInteger.valueOf(perThousand), RATE_DECIMALS) + " sat/vB, the Ledger shows the total",
                "coin     " + bitcoins(coin) + " BTC, " + Text.hex(txid).substring(2) + ":" + vout);
    }

    // Convert satoshis to a human-readable Bitcoin amount
    public static String bitcoins(long sats) {
        return Text.amount(BigInteger.valueOf(sats), DECIMALS);
    }

    // Convert a human-readable Bitcoin amount to satoshis, ensuring it does not exceed the maximum allowed
    private static long sats(String name, BigInteger value) {
        if (value.compareTo(BigInteger.valueOf(MAX_MONEY)) > 0) {
            throw new IllegalArgumentException(name + " is more than all bitcoin");
        }
        return value.longValueExact();
    }
}
