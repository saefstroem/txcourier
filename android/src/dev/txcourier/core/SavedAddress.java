package dev.txcourier.core;

public record SavedAddress(String label, String address) {
    @Override
    public String toString() {
        return label + "  " + address;
    }
}
