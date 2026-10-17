package dev.txcourier.core;

import java.io.IOException;
import java.math.BigInteger;
import java.util.function.BiConsumer;

public final class Node {
    private Node() {
    }

    // Suggests appropriate gas fees for a transaction on the given chain using the provided RPC interface.
    public static BigInteger[] suggest(Rpc rpc, Chain chain) throws IOException {
        String failure = "no node is known for " + chain;
        for (String url : chain.rpc()) {
            try {
                return rpc.fees(url);
            } catch (Throwable unusable) {
                failure = Text.reason(unusable);
            }
        }
        throw new IOException("could not fetch fees (" + failure + "), enter them by hand");
    }

    // Raises the suggested priority fee by 50% for faster inclusion of the transaction.
    public static BigInteger raise(BigInteger quote) {
        return quote.multiply(BigInteger.valueOf(3)).add(BigInteger.ONE).shiftRight(1);
    }

    // Calculates the maximum fee for a transaction given the base fee and the priority tip.
    public static BigInteger maxFee(BigInteger baseFee, BigInteger tip) {
        return baseFee.shiftLeft(1).add(tip);
    }

    // Publishes a signed transaction to the network using the provided RPC interface, reporting progress and errors.
    public static String publish(Rpc rpc, Chain chain, byte[] signed, BiConsumer<String, Throwable> report) throws IOException {
        boolean doubt = false;
        for (String url : chain.rpc()) {
            try {
                String hash = rpc.send(url, Text.hex(signed));
                if (hash.matches("0x[0-9a-fA-F]{64}")) {
                    return ":) published, " + chain.explorer() + hash;
                }
                doubt = true;
                report.accept(url, new IOException("node answered " + Text.clean(hash) + " instead of a transaction hash"));
            } catch (Refused refused) {
                doubt |= Text.reason(refused).matches("(?is).*(already known|known transaction|already imported).*");
                report.accept(url, refused);
            } catch (Throwable failure) {
                doubt = true;
                report.accept(url, failure);
            }
        }
        if (doubt) {
            throw new IOException("no node confirmed the transaction, but it may have gone out: check your account on the block"
                    + " explorer before signing a replacement; publishing this one again is safe");
        }
        throw new IOException("no node accepted the transaction");
    }
}
