package dev.txcourier.core;

import java.io.IOException;

public final class Refused extends IOException {
    private static final long serialVersionUID = 1L;

    public Refused(String message) {
        super(message);
    }
}
