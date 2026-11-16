package dev.txcourier.core;

import java.io.IOException;
import java.math.BigInteger;

public interface Rpc {
    BigInteger[] fees(String url) throws IOException;

    String send(String url, String signed) throws IOException;
}
