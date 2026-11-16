package dev.txcourier.core;

public record UnspentOutput(String txid, int vout, long sats, boolean confirmed) {
    @Override
    // Serialization of the unspent output into a human-readable string format.
    public String toString() {
        return Bitcoin.bitcoins(sats) + " BTC  " + txid + ":" + vout + (confirmed ? "" : "  unconfirmed");
    }
}
