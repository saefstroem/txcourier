import dev.txcourier.core.AddressBook;
import dev.txcourier.core.Bitcoin;
import dev.txcourier.core.Chain;
import dev.txcourier.core.Courier;
import dev.txcourier.core.Node;
import dev.txcourier.core.Refused;
import dev.txcourier.core.SavedAddress;
import dev.txcourier.core.Text;
import dev.txcourier.core.Token;
import dev.txcourier.core.Tx;
import dev.txcourier.core.UnspentOutput;
import java.io.IOException;
import java.math.BigInteger;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashSet;
import java.util.HexFormat;
import java.util.List;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

public final class CoreCheck {
    static final String RECIPIENT = "0xd8da6bf26964af9d7eed9e03e53415d37aa96045";
    static final Chain ETHEREUM = Chain.find(BigInteger.ONE);
    static final Chain BSC = Chain.find(BigInteger.valueOf(56));
    static final String NATIVE_RECORD = "000000000000000100000000000000000000000000000000000000003b9aca00000000000000000000000004a817c80000000000000052084554480000000000d8da6bf26964af9d7eed9e03e53415d37aa960450000000000000000000000000000000100";
    private static final String TOKEN_RECORD = "0000000000000001000000000000000700000000000000000000000059682f00000000000000000000000006fc23ac00000000000000fde85553444300000000d8da6bf26964af9d7eed9e03e53415d37aa9604500000000000000000000000000bc614e06";
    static final List<String> NATIVE_LINES = List.of(
            "send native    1 ETH",
            "to       0xd8da6bf26964af9d7eed9e03e53415d37aa96045",
            "chain    Ethereum (1)",
            "nonce    0",
            "gas      limit 21000, max fee 20 gwei, priority fee 1 gwei",
            "fees     at most 0.00042 ETH");
    private static final String BSC_NATIVE = "02f87338831a2ec5840393870084039387008261a894445a0d5b48fcad5af240a72f925609e498ec306a860d48cdb5090080c080a07a6c345139a2c0b62d1d39b1c44fe1871f5e17d5eaddfd19e06ab33e46618fb7a05f7baad2066db1ba75177726c2a5cea918caca4d305548228ae47617d313ca18";
    private static final String BSC_NATIVE_HASH = "0xfad9f10a64b130c46a907415fe0edb9d564d0c932bd1c2625566a3696bc3a9c1";
    private static final String BITCOIN_RECORD = "626331716372387465346b723630396763617775746d727a61306a34787638306a79387a333036667975000000000001d4c000000000000013881f1e1d1c1b1a191817161514131211100f0e0d0c0b0a090807060504030201000000000100000000000249f06263317177353038643671656a7874646734793572337a6172766172793063357877376b763866337434";
    private static final String FIRST = "https://first.example";
    private static final String SECOND = "https://second.example";

    public static void main(String[] args) throws Exception {
        transactions();
        amounts();
        fees();
        nonces();
        publishing();
        courier();
        book();
        balances();
        tokens(args[0]);
        bitcoin();
    }

    private static void transactions() {
        byte[] coin = Tx.build(ETHEREUM, "ETH", RECIPIENT, "1", "0", "1", "20", "");
        Check.that(Text.hex(coin).equals("0x" + NATIVE_RECORD), "a coin transfer becomes the record the courier decodes: " + Text.hex(coin));
        Check.that(Tx.describe(coin).equals(NATIVE_LINES), "the review shows what the record holds: " + Tx.describe(coin));
        byte[] token = Tx.build(ETHEREUM, "USDC", RECIPIENT, "12.345678", "7", "1.5", "30", "65000");
        Check.that(Text.hex(token).equals("0x" + TOKEN_RECORD), "a token transfer becomes the record the courier decodes: " + Text.hex(token));
        Check.that(Tx.describe(token).equals(List.of(
                "send     12.345678 USDC",
                "to       0xd8da6bf26964af9d7eed9e03e53415d37aa96045",
                "chain    Ethereum (1)",
                "nonce    7",
                "gas      limit 65000, max fee 30 gwei, priority fee 1.5 gwei",
                "fees     at most 0.00195 ETH")), "a token reads in its own units: " + Tx.describe(token));
        Check.that(gas(Tx.build(BSC, "USDT", RECIPIENT, "1", "0", "1", "20", " ")) == 150_000, "an empty gas limit means 150000 for a token");
        Check.that(gas(Tx.build(BSC, "BNB", RECIPIENT, "1", "0", "1", "20", "")) == 21_000, "an empty gas limit means 21000 for the chain's own coin");
        Check.that(gas(Tx.build(BSC, "ETH", RECIPIENT, "1", "0", "1", "20", "")) == 150_000, "ETH on BNB Smart Chain is not its coin");
        Check.that(Text.hex(Tx.build(ETHEREUM, "ETH", RECIPIENT, "100", "0", "1", "20", "")).endsWith(String.format("%032x%02x", 100, 0)), "a whole amount has no decimal places");
        Check.that(Text.hex(Tx.build(ETHEREUM, "ETH", RECIPIENT, "001.500", "0", "1", "20", "")).endsWith(String.format("%032x%02x", 15, 1)), "zeros that change nothing are not sent");
        Check.that(Tx.describe(Tx.build(ETHEREUM, "PAXG", RECIPIENT, "0.000000000000000001", "0", "1", "20", "")).get(0).equals("send     0.000000000000000001 PAXG"), "the finest amount the courier takes reads back");
        Check.that(Tx.build(ETHEREUM, "ETH", RECIPIENT, "340282366920938463463.374607431768211455", "18446744073709551615", "1", "20", "").length == Tx.RECORD_LEN, "the largest amount and nonce the courier takes still fit");

        Check.that(Check.failure(() -> Tx.build(ETHEREUM, "ETH", "0x" + "00".repeat(20), "1", "0", "1", "20", "")).equals("zero address would burn what is sent"), "the zero address is refused");
        Check.that(Check.failure(() -> Tx.build(ETHEREUM, "ETH", "0x1234", "1", "0", "1", "20", "")).equals("'0x1234' is not 0x and 40 hex digits"), "a short address is refused");
        Check.that(Check.failure(() -> Tx.build(ETHEREUM, "ETH", RECIPIENT, "0.0", "0", "1", "20", "")).equals("amount must be more than zero"), "a zero amount is refused");
        Check.that(Check.failure(() -> Tx.build(ETHEREUM, "ETH", RECIPIENT, "1", "0", "2", "1", "")).startsWith("max fee must be"), "a max fee below the tip is refused");
        Check.that(Check.failure(() -> Tx.build(ETHEREUM, "ETH", RECIPIENT, "1", "0", "1", "20", "20999")).equals("gas limit must be at least 21000"), "a gas limit below 21000 is refused");
        Check.that(Check.failure(() -> Tx.build(ETHEREUM, "ETH", RECIPIENT, "1", "1.5", "1", "20", "")).equals("nonce: '1.5' has more than 0 decimal places"), "a fractional nonce is refused");
        Check.that(Check.failure(() -> Tx.build(ETHEREUM, "ETH", RECIPIENT, "ten", "0", "1", "20", "")).equals("amount: 'ten' is not a number"), "a word is not an amount");
        Check.that(Check.failure(() -> Tx.build(ETHEREUM, "ETH", RECIPIENT, "0.0000000000000000001", "0", "1", "20", "")).equals("amount: '0.0000000000000000001' has more than 18 decimal places"), "an amount finer than any asset of the courier is refused");
        Check.that(Check.failure(() -> Tx.build(ETHEREUM, "ETH", RECIPIENT, "340282366920938463463.374607431768211456", "0", "1", "20", "")).equals("amount is too large for the courier"), "an amount above 128 bits is refused");
        Check.that(Check.failure(() -> Tx.build(ETHEREUM, "ETH", RECIPIENT, "1", "18446744073709551616", "1", "20", "")).equals("nonce is too large for the courier"), "a nonce above 64 bits is refused");
    }

    private static long gas(byte[] record) {
        return Long.parseLong(Text.hex(record).substring(2 + 2 * 48, 2 + 2 * 56), 16);
    }

    private static void amounts() {
        Check.that(Text.amount("1", 18).equals(BigInteger.TEN.pow(18)), "one coin is 10^18 base units");
        Check.that(Text.amount("12.5", 6).equals(BigInteger.valueOf(12_500_000)), "12.5 with six decimals");
        Check.that(Text.amount("12,5", 6).equals(BigInteger.valueOf(12_500_000)), "a decimal comma is a decimal point");
        Check.that(Text.amount("0.000001", 6).equals(BigInteger.ONE), "the smallest unit");
        Check.that(Text.amount(".5", 6).equals(BigInteger.valueOf(500_000)) && Text.amount("7.", 6).equals(BigInteger.valueOf(7_000_000)), "a missing whole or fraction part is zero");
        Check.that(Text.amount("  3  ", 0).equals(BigInteger.valueOf(3)), "surrounding spaces are ignored");
        Check.that(Text.amount(BigInteger.TWO.pow(256).subtract(BigInteger.ONE).toString(), 0).bitLength() == 256, "the largest 256-bit amount parses");
        Check.that(Check.failure(() -> Text.amount(BigInteger.TWO.pow(256).toString(), 0)).endsWith("is too large"), "2^256 is too large");
        Check.that(Check.failure(() -> Text.amount("0.0000001", 6)).equals("'0.0000001' has more than 6 decimal places"), "too many decimal places are refused");
        for (String junk : new String[] {"", ".", "-1", "+1", "1e3", "1.2.3", "1,000.5", "0x10", "one", " ", "1 2", "١"}) {
            Check.that(Check.failure(() -> Text.amount(junk, 18)).endsWith("is not a number"), "junk is not an amount: '" + junk + "'");
        }
        Check.that(Text.amount(BigInteger.TEN.pow(18), 18).equals("1") && Text.amount(BigInteger.valueOf(12_500_000), 6).equals("12.5"), "amounts print without trailing zeros");
        Check.that(Text.amount(BigInteger.ONE, 6).equals("0.000001") && Text.amount(BigInteger.ZERO, 6).equals("0"), "small amounts and zero print plainly");
        Check.that(Text.amount(BigInteger.valueOf(1000), 0).equals("1000") && Text.amount(BigInteger.valueOf(1_234_567_891L), 9).equals("1.234567891"), "whole numbers and gwei print plainly");
        for (long value : new long[] {0, 1, 7, 12_500_000, 999_999_999, 1_000_000_000_000_000_001L}) {
            Check.that(Text.amount(Text.amount(BigInteger.valueOf(value), 18), 18).equals(BigInteger.valueOf(value)), "printing and parsing agree: " + value);
        }
        Check.that(Text.clean("a\u001b[2K‮:) published\r\n").equals("a[2K:) published"), "control and bidi characters are dropped");
        Check.that(Text.clean("x".repeat(1000)).length() == 300, "shown text is capped");
        Check.that(Text.reason(new IOException("Unexpected status line: \u202e:) published\u001b")).equals("Unexpected status line: :) published"), "error text is cleaned before it is shown");
        Check.that(Text.reason(new IOException("y".repeat(5000))).length() == 300 && Text.reason(new IOException()).equals("IOException"), "error text is capped and never empty");
    }

    private static void fees() throws Exception {
        for (long quote = 0; quote <= 10_000; quote++) {
            expectRaise(BigInteger.valueOf(quote));
        }
        for (int bit = 0; bit < 256; bit++) {
            expectRaise(BigInteger.ONE.shiftLeft(bit));
            expectRaise(BigInteger.ONE.shiftLeft(bit).subtract(BigInteger.ONE));
        }
        Check.that(Node.raise(BigInteger.valueOf(1_000_000_000)).equals(BigInteger.valueOf(1_500_000_000)), "1 gwei is raised to 1.5 gwei");
        Check.that(Node.raise(BigInteger.ONE).equals(BigInteger.TWO) && Node.raise(BigInteger.valueOf(3)).equals(BigInteger.valueOf(5)), "odd quotes round up");
        Check.that(Node.maxFee(BigInteger.valueOf(7), BigInteger.valueOf(2)).equals(BigInteger.valueOf(16)), "the max fee is twice the base fee plus the tip");

        BigInteger[] quoted = {BigInteger.TEN, BigInteger.ONE};
        FakeRpc rpc = new FakeRpc().node(FIRST).node(SECOND, (Object) quoted);
        Check.that(Node.suggest(rpc, chain(FIRST, SECOND)) == quoted, "the first node that answers is used");
        Check.that(rpc.calls.equals(List.of("fees " + FIRST, "fees " + SECOND)), "a dead node is skipped: " + rpc.calls);
        Check.that(Node.suggest(new FakeRpc().node(FIRST, new StackOverflowError()).node(SECOND, (Object) quoted), chain(FIRST, SECOND)) == quoted, "a node that overflows the stack is skipped too");
        Check.that(Check.failure(() -> Node.suggest(new FakeRpc(), chain(FIRST))).equals("could not fetch fees (connection refused by " + FIRST + "), enter them by hand"), "the last failure is reported");
        Check.that(Check.failure(() -> Node.suggest(new FakeRpc(), chain())).equals("could not fetch fees (no node is known for BNB Smart Chain (56)), enter them by hand"), "a chain without nodes says so");
    }

    private static void book() {
        String stored = AddressBook.added(AddressBook.added("", "My Ledger", " 0xF5f107abd30e733FDB3355Fc07930410C425B7e7 "), "Savings\nline", "BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4");
        List<SavedAddress> book = AddressBook.read(stored);
        Check.that(book.equals(List.of(new SavedAddress("My Ledger", "0xf5f107abd30e733fdb3355fc07930410c425b7e7"),
                new SavedAddress("Savingsline", "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"))), "addresses are kept in order and normalized: " + book);
        Check.that(AddressBook.evm(book).equals(book.subList(0, 1)) && AddressBook.bitcoin(book).equals(book.subList(1, 2)), "each chain sees only its own addresses");
        Check.that(Check.failure(() -> AddressBook.added(stored, "Again", "0xf5f107abd30e733fdb3355fc07930410c425b7e7")).equals("this address is already saved as My Ledger"), "an address is saved once");
        Check.that(Check.failure(() -> AddressBook.added("", " ", RECIPIENT)).equals("a label is needed"), "a label is needed");
        Check.that(Check.failure(() -> AddressBook.added("", "Old", "1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2")).startsWith("'1BvBMSEYstWetqTFn5Au4m4GFg7xJaNVN2' is neither"), "other address kinds are refused");
        Check.that(AddressBook.read(AddressBook.removed(stored, book.get(0))).equals(book.subList(1, 2)), "a removed address is gone");
    }

    private static void nonces() throws Exception {
        FakeRpc rpc = new FakeRpc().node(FIRST, new IOException("down")).node(SECOND, BigInteger.valueOf(4), BigInteger.valueOf(4));
        Check.that(Node.checkNonce(rpc, chain(FIRST, SECOND), RECIPIENT, "4").equals(BigInteger.valueOf(4)), "the next nonce passes");
        Check.that(rpc.calls.equals(List.of("nonce " + FIRST, "nonce " + SECOND)), "a dead node is skipped: " + rpc.calls);
        Check.that(Node.checkNonce(rpc, chain(SECOND), RECIPIENT, "3").equals(BigInteger.valueOf(4)), "a used nonce is left for the nodes to refuse");
        FakeRpc fresh = new FakeRpc().node(FIRST, BigInteger.ZERO).node(SECOND, BigInteger.TEN);
        Check.that(Check.failure(() -> Node.checkNonce(fresh, chain(FIRST, SECOND), RECIPIENT, "1"))
                .equals("nonce 1 is too high: the next nonce of this address on BNB Smart Chain (56) is 0"), "a nonce nodes would hold back is refused");
        Check.that(fresh.calls.equals(List.of("nonce " + FIRST)), "no other node is asked to overrule the refusal: " + fresh.calls);
        Check.that(Check.failure(() -> Node.checkNonce(new FakeRpc(), chain(FIRST), RECIPIENT, "0")).equals("could not fetch the nonce (connection refused by " + FIRST + ")"), "no answer means no review");
        Check.that(Check.failure(() -> Node.checkNonce(new FakeRpc(), chain(FIRST), RECIPIENT, "")).equals("nonce: '' is not a number"), "an empty nonce is refused before a node is asked");
        FakeRpc fetched = new FakeRpc().node(FIRST, new IOException("down")).node(SECOND, BigInteger.valueOf(7));
        Check.that(Node.nonce(fetched, chain(FIRST, SECOND), RECIPIENT).equals(BigInteger.valueOf(7)), "the next nonce is fetched from the first node that answers");
    }

    private static void expectRaise(BigInteger quote) {
        BigInteger raised = Node.raise(quote);
        Check.that(raised.shiftLeft(1).compareTo(quote.multiply(BigInteger.valueOf(3))) >= 0, quote + " raised to " + raised + " is less than half again");
        Check.that(raised.signum() == 0 || raised.subtract(BigInteger.ONE).shiftLeft(1).compareTo(quote.multiply(BigInteger.valueOf(3))) < 0, quote + " raised to " + raised + " is more than half again rounded up");
    }

    private static void publishing() throws Exception {
        String link = "https://bscscan.com/tx/" + BSC_NATIVE_HASH;
        List<String> reported = new ArrayList<>();
        Check.that(publish(new FakeRpc().node(FIRST, BSC_NATIVE_HASH), reported, FIRST).equals(":) published, " + link), "a node that answers a transaction hash published it");
        Check.that(reported.isEmpty(), "nothing was reported");

        FakeRpc rpc = new FakeRpc().node(FIRST, new IOException("connection reset")).node(SECOND, BSC_NATIVE_HASH);
        Check.that(publish(rpc, reported, FIRST, SECOND).startsWith(":) published"), "the next node is tried");
        Check.that(reported.equals(List.of(FIRST + ": connection reset")), "the dead node is reported: " + reported);

        rpc = new FakeRpc().node(FIRST, new Refused("insufficient funds")).node(SECOND, new Refused("insufficient funds"));
        Check.that(publish(rpc, new ArrayList<>(), FIRST, SECOND).equals("no node accepted the transaction"), "clean refusals mean not published");
        Check.that(rpc.calls.size() == 2, "every node was asked once: " + rpc.calls);

        String unconfirmed = "no node confirmed the transaction, but it may have gone out: check your account on the block"
                + " explorer before signing a replacement; publishing this one again is safe";
        Check.that(publish(new FakeRpc().node(FIRST, new IOException("hung up")).node(SECOND, new Refused("nonce too low")), new ArrayList<>(), FIRST, SECOND).equals(unconfirmed), "a hang up leaves doubt");
        Check.that(publish(new FakeRpc().node(FIRST, new Refused("node refused: already known (code -32000)")).node(SECOND, new Refused("insufficient funds")), new ArrayList<>(), FIRST, SECOND).equals(unconfirmed), "a node that already knows it leaves doubt");
        Check.that(publish(new FakeRpc().node(FIRST, new StackOverflowError()), new ArrayList<>(), FIRST).equals(unconfirmed), "an unexpected error leaves doubt");
        reported.clear();
        Check.that(publish(new FakeRpc().node(FIRST, "null"), reported, FIRST).equals(unconfirmed), "an answer that is not a transaction hash leaves doubt");
        Check.that(reported.equals(List.of(FIRST + ": node answered null instead of a transaction hash")), "the odd answer is reported: " + reported);
    }

    private static String publish(FakeRpc rpc, List<String> reported, String... urls) {
        try {
            return Node.publish(rpc, chain(urls), Text.bytes(BSC_NATIVE), (url, failure) -> reported.add(url + ": " + Text.reason(failure)));
        } catch (IOException outcome) {
            return outcome.getMessage();
        }
    }

    private static Chain chain(String... urls) {
        return new Chain(56, "BNB Smart Chain", "BNB", List.of(urls), "https://bscscan.com/tx/", List.of());
    }

    private static void courier() throws Exception {
        ScriptedLink link = new ScriptedLink(Text.bytes("525701810400a1000000"));
        Check.that(new Courier(link).status().equals(List.of(
                "state   idle",
                "accepts an EIP-1559 transfer of ETH, BNB, USDT, USDC, PAXG, VTIon, IEFAon, IEMGon or NVDAon, or a bitcoin payment from one coin",
                "signing no failed attempt recorded")), "an idle status reads in words");
        Check.that(Text.hex(link.written.toByteArray()).equals("0x525701010000"), "a status request is one frame");
        List<String> declined = new Courier(new ScriptedLink(Text.bytes("525701810400b20e6985"))).status();
        Check.that(declined.get(0).equals("state   pending transaction stored"), "a pending transaction is named");
        Check.that(declined.get(2).equals("signing you declined on the Ledger (the Ledger answered 0x6985)"), "a refusal names the Ledger's answer");
        Check.that(new Courier(new ScriptedLink(Text.bytes("52570181040000000000"))).status().get(0).equals("state   fault (0x00)"), "the fault byte is shown in hex");
        Check.that(Check.failure(() -> courier("525701810500a100000000").status()).equals("courier's status is 5 bytes, not 4"), "a status from older firmware is refused");

        ScriptedLink put = new ScriptedLink(Text.bytes("525701880000"));
        new Courier(put).putEVM(Text.bytes(NATIVE_RECORD));
        Check.that(Text.hex(put.written.toByteArray()).equals("0x5257010e6500" + NATIVE_RECORD), "a put writes the record under its own opcode");
        ScriptedLink clear = new ScriptedLink(Text.bytes("525701880000"));
        new Courier(clear).clear(Courier.SIGNED);
        Check.that(Text.hex(clear.written.toByteArray()).equals("0x5257010d010001"), "a clear writes the slot selector");
        ScriptedLink get = new ScriptedLink(Text.bytes("525701870300aabbcc"));
        Check.that(Text.hex(new Courier(get).get(Courier.PENDING)).equals("0xaabbcc") && Text.hex(get.written.toByteArray()).equals("0x52570108010000"), "a get returns the slot");

        Check.that(Check.failure(() -> courier("5257018e0300020000").putEVM(new byte[1])).equals("courier refused: a pending transaction is already stored"), "a refusal is explained");
        String[][] refusals = {
                {"12", "that asset is not built in for that chain"},
                {"13", "amount has more decimal places than its asset"},
                {"14", "amount is more than 128 bits of base units"},
                {"15", "recipient is the token's own contract, and tokens sent there are usually lost"},
                {"16", "Bitcoin transaction is not one coin paying a 42-character bc1q address"},
                {"17", "coin does not cover the amount and the fee"},
                {"18", "pending transaction has no Ethereum or Bitcoin identifier"},
                {"19", "Bitcoin payment is under 294 sats, which nodes refuse to relay"}};
        for (String[] refusal : refusals) {
            Check.that(Check.failure(() -> courier("5257018e0300" + refusal[0] + "0000").putEVM(new byte[1])).equals("courier refused: " + refusal[1]), "refusal 0x" + refusal[0] + " is explained");
        }
        Check.that(Check.failure(() -> courier("5257018e03000a0000").putEVM(new byte[1])).equals("courier refused: an unknown refusal 0x0a"), "the retired token table refusal is unknown");
        Check.that(Check.failure(() -> courier("525701880000").status()).equals("courier answered with opcode 0x88, which does not match the request"), "an acknowledgement is not a status");
        Check.that(Check.failure(() -> courier("005701880000").status()).equals("courier's answer does not start like a frame"), "a bad header is refused");
        Check.that(Check.failure(() -> courier("52570181ffff").status()).equals("courier announced 65535 bytes, more than a frame holds"), "an oversized answer is refused before it is read");
        Check.that(Check.failure(() -> courier("5257018104").status()).equals("end of stream"), "a cut answer is a read failure");
        Check.that(Check.failure(() -> courier("").putEVM(new byte[2049])).equals("a request of 2049 bytes is too large for the courier"), "an oversized request is refused");
    }

    private static void balances() throws Exception {
        Chain chain = new Chain(1, "Ethereum", "ETH", List.of(FIRST, SECOND), "", ETHEREUM.tokens().subList(0, 3));
        FakeRpc rpc = new FakeRpc().node(FIRST, new IOException("down"))
                .node(SECOND, BigInteger.valueOf(15).multiply(BigInteger.TEN.pow(17)), BigInteger.valueOf(12_500_000), BigInteger.ZERO, BigInteger.ONE);
        Check.that(Node.balances(rpc, chain, RECIPIENT).equals(List.of("1.5 ETH", "12.5 USDT", "0 USDC", "0.000000000000000001 PAXG")), "balances read in each asset's own units");
        Check.that(rpc.calls.equals(List.of("balance " + FIRST, "balance " + SECOND,
                "token 0xdAC17F958D2ee523a2206206994597C13D831ec7 " + SECOND,
                "token 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48 " + SECOND,
                "token 0x45804880De22913dAFE09f4980848ECE6EcbAf78 " + SECOND)), "a dead node is skipped and every token is asked of one node: " + rpc.calls);
        Check.that(Check.failure(() -> Node.balances(new FakeRpc(), chain(FIRST), RECIPIENT)).equals("could not fetch balances (connection refused by " + FIRST + ")"), "the last failure is reported");
    }

    private static void tokens(String knownRs) throws IOException {
        Matcher entry = Pattern.compile("chain_id: (\\d+),\\s+ticker: \"(\\w+)\",\\s+decimals: (\\d+),\\s+kind: Kind::Token \\{\\s+contract: address\\(\"(0x[0-9a-fA-F]{40})\"\\)")
                .matcher(Files.readString(Path.of(knownRs)));
        Set<String> courier = new HashSet<>();
        while (entry.find()) {
            courier.add(entry.group(1) + " " + entry.group(2) + " " + entry.group(3) + " " + entry.group(4));
        }
        Set<String> app = new HashSet<>();
        for (Chain chain : Chain.KNOWN) {
            for (Token token : chain.tokens()) {
                app.add(chain.id() + " " + token.symbol() + " " + token.decimals() + " " + token.contract());
            }
        }
        Check.that(courier.size() == 9 && app.equals(courier), "the app's token contracts and decimals are the courier's: " + app + " " + courier);
    }

    private static void bitcoin() throws Exception {
        String from = "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu";
        String to = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
        String txid = "1f1e1d1c1b1a191817161514131211100f0e0d0c0b0a09080706050403020100";
        UnspentOutput coin = new UnspentOutput(txid, 1, 150_000, false);
        byte[] record = Bitcoin.build(from, coin, to, "0.0012", "5");
        Check.that(Text.hex(record).equals("0x" + BITCOIN_RECORD), "a bitcoin payment becomes the record the courier decodes: " + Text.hex(record));
        Check.that(Bitcoin.describe(record).equals(List.of(
                "send     0.0012 BTC",
                "to       " + to,
                "change   any rest back to " + from,
                "fee      5 sat/vB, the Ledger shows the total",
                "coin     0.0015 BTC, " + txid + ":1")), "the review shows what the record holds: " + Bitcoin.describe(record));
        byte[] sweep = Bitcoin.build(from, coin, to, "", "2.5");
        Check.that(Text.hex(Arrays.copyOfRange(sweep, 42, 50)).equals("0xffffffffffffffff"), "an empty amount asks the courier for the whole coin");
        Check.that(Bitcoin.describe(sweep).subList(0, 4).equals(List.of(
                "send     whole coin, less the fee",
                "to       " + to,
                "change   none",
                "fee      2.5 sat/vB, the Ledger shows the total")), "the review of a whole coin names no amount: " + Bitcoin.describe(sweep));
        Check.that(Check.failure(() -> Bitcoin.build(from, coin, to, "0", "5")).equals("amount must be more than zero"), "a zero amount is refused, never read as the whole coin");
        Check.that(Check.failure(() -> Bitcoin.build(from, coin, to, "0.0001", "0")).equals("fee rate must be more than zero"), "a free transaction is refused");
        for (String odd : new String[] {"bc1q", "bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0", "bc1qrp33g0q5c5txsp9arysrx4k6zdkfs4nce4xj0gdcccefvpysxf3qccfmv3", "0xF5f107abd30e733FDB3355Fc07930410C425B7e7"}) {
            Check.that(Check.failure(() -> Bitcoin.build(from, coin, odd, "0.0001", "5")).equals("'" + odd + "' is not a 42-character bc1q address"), "only a regular bc1q recipient reaches the courier: " + odd);
        }
        Check.that(Check.failure(() -> Bitcoin.build(from, coin, to, "21000000.00000001", "5")).equals("amount is more than all bitcoin"), "an amount past the supply is refused");
        Check.that(Check.failure(() -> Bitcoin.build(from, coin, to, "", "99999999999999")).equals("fee rate is more than all bitcoin"), "a fee rate past the supply is refused");
        Check.that(Text.clean("0xabc\u2028:) published\u200b\u00ad").equals("0xabc:) published"), "line and format separators are stripped from server text");
        Check.that(coin.toString().equals("0.0015 BTC  " + txid + ":1  unconfirmed"), "a coin reads as its value and outpoint");

        ScriptedLink put = new ScriptedLink(Text.bytes("525701880000"));
        new Courier(put).putBitcoin(record);
        Check.that(Text.hex(put.written.toByteArray()).equals("0x525701139000" + BITCOIN_RECORD), "a bitcoin put writes the record under its own opcode");
    }

    private static String ascii(String text) {
        return HexFormat.of().formatHex(text.getBytes(StandardCharsets.US_ASCII));
    }

    private static Courier courier(String reply) {
        return new Courier(new ScriptedLink(Text.bytes(reply)));
    }
}
