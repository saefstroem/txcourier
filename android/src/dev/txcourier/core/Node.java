package dev.txcourier.core;

import java.io.IOException;
import java.math.BigInteger;
import java.util.ArrayList;
import java.util.List;
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

    public static List<String> balances(Rpc rpc, Chain chain, String address) throws IOException {
        String failure = "no node is known for " + chain;
        for (String url : chain.rpc()) {
            try {
                List<String> lines = new ArrayList<>();
                lines.add(Text.amount(rpc.balance(url, address), Tx.COIN_DECIMALS) + " " + chain.symbol());
                for (Token token : chain.tokens()) {
                    lines.add(Text.amount(rpc.tokenBalance(url, token.contract(), address), token.decimals()) + " "
                            + token.symbol());
                }
                return lines;
            } catch (Throwable unusable) {
                failure = Text.reason(unusable);
            }
        }
        throw new IOException("could not fetch balances (" + failure + ")");
    }

    public static BigInteger checkNonce(Rpc rpc, Chain chain, String address, String typed) throws IOException {
        BigInteger nonce = Tx.parsed("nonce", typed, 0);
        BigInteger next = nonce(rpc, chain, address);
        if (nonce.compareTo(next) > 0) {
            throw new IOException("nonce " + nonce + " is too high: the next nonce of this address on " + chain + " is " + next);
        }
        return next;
    }

    public static BigInteger nonce(Rpc rpc, Chain chain, String address) throws IOException {
        String failure = "no node is known for " + chain;
        for (String url : chain.rpc()) {
            try {
                return rpc.nonce(url, address);
            } catch (Throwable unusable) {
                failure = Text.reason(unusable);
            }
        }
        throw new IOException("could not fetch the nonce (" + failure + ")");
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
