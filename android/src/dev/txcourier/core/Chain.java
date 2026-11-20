package dev.txcourier.core;

import java.math.BigInteger;
import java.util.List;

public record Chain(long id, String name, String symbol, List<String> rpc, String explorer, List<Token> tokens) {
    // A list of known blockchain networks with their respective RPC endpoints and explorers.
    public static final List<Chain> KNOWN = List.of(
            new Chain(1, "Ethereum", "ETH",
                    List.of("https://1rpc.io/eth", "https://eth.drpc.org", "https://rpc.flashbots.net"),
                    "https://etherscan.io/tx/",
                    List.of(new Token("USDT", "0xdAC17F958D2ee523a2206206994597C13D831ec7", 6),
                            new Token("USDC", "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", 6),
                            new Token("EURC", "0x1aBaEA1f7C830bD89Acc67eC4af516284b1bC33c", 6),
                            new Token("PAXG", "0x45804880De22913dAFE09f4980848ECE6EcbAf78", 18),
                            new Token("VTIon", "0x57B392146848C6321bb2A3D4358DF1bDEACdc62A", 18),
                            new Token("IEFAon", "0xFEFf7a377A86462F5a2A872009722C154707F09e", 18),
                            new Token("IEMGon", "0xcDD60D15125bf3362b6838D2506b0Fa33bc1a515", 18),
                            new Token("NVDAon", "0x2D1F7226Bd1F780AF6B9A49DCC0aE00E8Df4bDEE", 18))),
            new Chain(56, "BNB Smart Chain", "BNB",
                    List.of("https://1rpc.io/bnb", "https://bsc-dataseed.bnbchain.org", "https://bsc-rpc.publicnode.com"),
                    "https://bscscan.com/tx/",
                    List.of(new Token("USDT", "0x55d398326f99059fF775485246999027B3197955", 18),
                            new Token("USDC", "0x8AC76a51cc950d9822D68b83fE1Ad97B32Cd580d", 18))));

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
