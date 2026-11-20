package dev.txcourier.core;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.util.List;

public final class Courier {
    // Slot numbers for the courier's transaction storage.
    // PENDING is the slot for a pending transaction.
    // SIGNED is the slot for a signed transaction.
    public static final int PENDING = 0;
    public static final int SIGNED = 1;
    public static final byte ETHEREUM_IDENTIFIER = 0x37;
    public static final byte BITCOIN_IDENTIFIER = (byte) 0x87;

    // Command codes for the courier protocol.
    private static final int STATUS = 0x01;
    private static final int PUT = 0x0E;
    private static final int GET = 0x08;
    private static final int CLEAR = 0x0D;
    private static final int PUT_BITCOIN = 0x13;
    private static final int STATUS_REPLY = 0x81;
    private static final int SLOT_REPLY = 0x87;
    private static final int DONE = 0x88;
    private static final int REFUSED = 0x8E;
    private static final int VERSION = 1;
    private static final int HEADER_LEN = 6;
    private static final int MAX_PAYLOAD = 2048;
    private static final int BRISK_MILLIS = 3_000;

    // The raw link to the courier device.
    private final Link link;

    public Courier(Link link) {
        this.link = link;
    }

    // Retrieves the current status of the courier, including its state and signing information.
    public List<String> status() throws IOException {
        byte[] status = ask(STATUS, new byte[0], STATUS_REPLY, BRISK_MILLIS);
        if (status.length != 4) {
            throw new IOException("courier's status is " + status.length + " bytes, not 4");
        }
        String state = switch (status[0] & 0xFF) {
            case 0xA1 -> "idle";
            case 0xB2 -> "pending transaction stored";
            case 0xC3 -> "signed transaction stored";
            default -> String.format("fault (0x%02x)", status[0] & 0xFF);
        };
        return List.of(
                "state   " + state,
                "accepts an EIP-1559 transfer of ETH, BNB, USDT, USDC, EURC, PAXG, VTIon, IEFAon, IEMGon or NVDAon, or a bitcoin payment from one coin",
                "signing " + (status[1] == 0 ? "no failed attempt recorded" : refusal(status, 1)));
    }

    // Stores an unsigned EVM transaction in the courier's pending slot.
    public void putEVM(byte[] unsigned) throws IOException {
        ask(PUT, unsigned, DONE, BRISK_MILLIS);
    }

    // Retrieves a transaction from the specified slot (PENDING or SIGNED).
    public byte[] get(int slot) throws IOException {
        return ask(GET, new byte[] {(byte) slot}, SLOT_REPLY, BRISK_MILLIS);
    }

    // Clears the specified slot (PENDING or SIGNED) in the courier's storage.
    public void clear(int slot) throws IOException {
        ask(CLEAR, new byte[] {(byte) slot}, DONE, BRISK_MILLIS);
    }

    // Stores an unsigned Bitcoin transaction in the courier's pending slot.
    public void putBitcoin(byte[] record) throws IOException {
        ask(PUT_BITCOIN, record, DONE, BRISK_MILLIS);
    }

    // Sends a request to the courier and waits for the expected reply with a specified timeout.
    private byte[] ask(int request, byte[] payload, int expected, int millis) throws IOException {
        if (payload.length > MAX_PAYLOAD) {
            throw new IOException("a request of " + payload.length + " bytes is too large for the courier");
        }
        ByteArrayOutputStream frame = new ByteArrayOutputStream();
        // Construct the frame to send to the courier, including the header and payload.
        frame.writeBytes(new byte[] {'R', 'W', VERSION, (byte) request, (byte) payload.length, (byte) (payload.length >> 8)});
        frame.writeBytes(payload);
        link.write(frame.toByteArray());
        // Read and validate the header of the courier's response.
        byte[] header = link.read(HEADER_LEN, millis);
        if (header[0] != 'R' || header[1] != 'W' || header[2] != VERSION) {
            throw new IOException("courier's answer does not start like a frame");
        }
        int length = header[4] & 0xFF | (header[5] & 0xFF) << 8;
        if (length > MAX_PAYLOAD) {
            throw new IOException("courier announced " + length + " bytes, more than a frame holds");
        }
        // Read the payload of the courier's response.
        byte[] answer = link.read(length, millis);
        int reply = header[3] & 0xFF;
        if (reply == REFUSED && answer.length == 3) {
            throw new Refused("courier refused: " + refusal(answer, 0));
        }
        if (reply != expected) {
            throw new IOException(String.format("courier answered with opcode 0x%02x, which does not match the request", reply));
        }
        return answer;
    }

    // Interprets the refusal code from the courier's response and returns a human-readable reason.
    private static String refusal(byte[] bytes, int at) {
        String reason = switch (bytes[at] & 0xFF) {
            case 1 -> "both slots are occupied, which must never happen";
            case 2 -> "a pending transaction is already stored";
            case 3 -> "a signed transaction is waiting to be collected";
            case 4 -> "there is no pending transaction";
            case 5 -> "there is no signed transaction";
            case 6 -> "transaction is not a well-formed EIP-1559 envelope";
            case 7 -> "transaction is not a plain transfer";
            case 11 -> "payload is larger than the device can store";
            case 12 -> "storage sector cannot take another record";
            case 13 -> "no Ledger answered on the USB-A port";
            case 14 -> "you declined on the Ledger";
            case 15 -> "Ledger did not complete the request";
            case 16 -> "Ledger was not approved in time";
            case 17 -> "that token has no Ledger-signed descriptor, so it could only be blind signed";
            case 18 -> "that asset is not built in for that chain";
            case 19 -> "amount has more decimal places than its asset";
            case 20 -> "amount is more than 128 bits of base units";
            case 21 -> "recipient is the token's own contract, and tokens sent there are usually lost";
            case 22 -> "Bitcoin transaction is not one coin paying a 42-character bc1q address";
            case 23 -> "coin does not cover the amount and the fee";
            case 24 -> "pending transaction has no Ethereum or Bitcoin identifier";
            case 25 -> "Bitcoin payment is under 294 sats, which nodes refuse to relay";
            default -> String.format("an unknown refusal 0x%02x", bytes[at] & 0xFF);
        };
        int detail = (bytes[at + 1] & 0xFF) << 8 | bytes[at + 2] & 0xFF;
        return detail == 0 ? reason : String.format("%s (the Ledger answered 0x%04x)", reason, detail);
    }
}
