package dev.txcourier.core;

import java.io.IOException;
import java.math.BigInteger;

public interface Rpc {
    BigInteger[] fees(String url) throws IOException;

    String send(String url, String signed) throws IOException;

    BigInteger balance(String url, String address) throws IOException;

    BigInteger tokenBalance(String url, String contract, String address) throws IOException;
}
