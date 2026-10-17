package dev.txcourier.core;

import java.io.IOException;

public interface Link {
    void write(byte[] bytes) throws IOException;

    byte[] read(int length) throws IOException;
}
