package dev.txcourier.core;

import java.math.BigInteger;
import java.util.List;

public record Chain(long id, String name, String symbol, List<String> rpc, String explorer) {
    // A list of known blockchain networks with their respective RPC endpoints and explorers.
    public static final List<Chain> KNOWN = List.of(
            new Chain(1, "Ethereum", "ETH",
                    List.of("https://1rpc.io/eth", "https://eth.drpc.org", "https://rpc.flashbots.net"),
                    "https://etherscan.io/tx/"),
            new Chain(56, "BNB Smart Chain", "BNB",
                    List.of("https://1rpc.io/bnb", "https://bsc-dataseed.bnbchain.org", "https://bsc-rpc.publicnode.com"),
                    "https://bscscan.com/tx/"));

    // Finds a chain by its ID, returning null if not found.
    public static Chain find(BigInteger id) {
        return KNOWN.stream().filter(chain -> BigInteger.valueOf(chain.id).equals(id)).findFirst().orElse(null);
    }

    // Returns a human-readable description of the chain, or a default string if not found.
    public static String describe(BigInteger id) {
        Chain chain = find(id);
        return chain == null ? "chain " + id : chain.toString();
    }

    @Override
    public String toString() {
        return name + " (" + id + ")";
    }
}
