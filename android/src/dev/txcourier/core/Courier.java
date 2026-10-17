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

    // Command codes for the courier protocol.
    private static final int STATUS = 0x01;
    private static final int PUT = 0x0E;
    private static final int GET = 0x08;
    private static final int CLEAR = 0x0D;
    private static final int STATUS_REPLY = 0x81;
    private static final int SLOT_REPLY = 0x87;
    private static final int DONE = 0x88;
    private static final int REFUSED = 0x8E;
    private static final int VERSION = 1;
    private static final int HEADER_LEN = 6;
    private static final int MAX_PAYLOAD = 2048;

    // The raw link to the courier device.
    private final Link link;

    public Courier(Link link) {
        this.link = link;
    }

    // Retrieves the current status of the courier, including its state and signing information.
    public List<String> status() throws IOException {
        byte[] status = ask(STATUS, new byte[0], STATUS_REPLY);
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
                "accepts an EIP-1559 transfer of ETH, BNB, USDT, USDC or PAXG to any recipient",
                "signing " + (status[1] == 0 ? "no failed attempt recorded" : refusal(status, 1)));
    }

    // Stores an unsigned transaction in the courier's pending slot.
    public void put(byte[] unsigned) throws IOException {
        ask(PUT, unsigned, DONE);
    }

    // Retrieves a transaction from the specified slot (PENDING or SIGNED).
    public byte[] get(int slot) throws IOException {
        return ask(GET, new byte[] {(byte) slot}, SLOT_REPLY);
    }

    // Clears the specified slot (PENDING or SIGNED) in the courier's storage.
    public void clear(int slot) throws IOException {
        ask(CLEAR, new byte[] {(byte) slot}, DONE);
    }

    // Sends a request to the courier and waits for the expected reply.
    private byte[] ask(int request, byte[] payload, int expected) throws IOException {
        if (payload.length > MAX_PAYLOAD) {
            throw new IOException("a request of " + payload.length + " bytes is too large for the courier");
        }
        ByteArrayOutputStream frame = new ByteArrayOutputStream();
        // Construct the frame to send to the courier, including the header and payload.
        frame.writeBytes(new byte[] {'R', 'W', VERSION, (byte) request, (byte) payload.length, (byte) (payload.length >> 8)});
        frame.writeBytes(payload);
        link.write(frame.toByteArray());
        // Read and validate the header of the courier's response.
        byte[] header = link.read(HEADER_LEN);
        if (header[0] != 'R' || header[1] != 'W' || header[2] != VERSION) {
            throw new IOException("courier's answer does not start like a frame");
        }
        int length = header[4] & 0xFF | (header[5] & 0xFF) << 8;
        if (length > MAX_PAYLOAD) {
            throw new IOException("courier announced " + length + " bytes, more than a frame holds");
        }
        // Read the payload of the courier's response.
        byte[] answer = link.read(length);
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
            case 14 -> "you declined the transaction on the Ledger";
            case 15 -> "Ledger did not complete the signature";
            case 16 -> "Ledger was not approved in time";
            case 17 -> "that token has no Ledger-signed descriptor, so it could only be blind signed";
            case 18 -> "that asset is not built in for that chain";
            case 19 -> "amount has more decimal places than its asset";
            case 20 -> "amount is more than 128 bits of base units";
            case 21 -> "recipient is the token's own contract, and tokens sent there are usually lost";
            default -> String.format("an unknown refusal 0x%02x", bytes[at] & 0xFF);
        };
        int detail = (bytes[at + 1] & 0xFF) << 8 | bytes[at + 2] & 0xFF;
        return detail == 0 ? reason : String.format("%s (the Ledger answered 0x%04x)", reason, detail);
    }
}
