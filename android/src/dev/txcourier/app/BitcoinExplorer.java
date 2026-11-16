package dev.txcourier.app;

import dev.txcourier.core.Bitcoin;
import dev.txcourier.core.Text;
import dev.txcourier.core.UnspentOutput;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

final class BitcoinExplorer {
    private static final String MEMPOOL = "https://mempool.space/api/address/";
    private static final String BROADCAST = "https://mempool.space/api/tx";
    private static final String PUBLISHED = "https://mempool.space/tx/";

    private BitcoinExplorer() {
    }

    // Returns the confirmed and unconfirmed balance of the given address in BTC
    static String balance(String address) throws IOException, JSONException {
        // Fetch the address summary from the mempool.space API
        if (!(HttpRpc.get(MEMPOOL + address) instanceof JSONObject summary)) {
            throw new IOException("explorer's address summary is not an object");
        }
        // Extract the confirmed and unconfirmed balances from the summary
        long confirmed = held(summary.getJSONObject("chain_stats"));
        // extract the unconfirmed balance from the mempool stats
        long unconfirmed = held(summary.getJSONObject("mempool_stats"));
        // return the balance as a string in BTC, including unconfirmed if any
        return Bitcoin.bitcoins(confirmed) + " BTC" + (unconfirmed == 0 ? "" : ", and " + Bitcoin.bitcoins(unconfirmed) + " BTC unconfirmed");
    }

    /// Returns the list of unspent outputs (UTXOs) for the given address in the mempool.space API
    static List<UnspentOutput> coins(String address) throws IOException, JSONException {
        // Fetch the list of unspent outputs from the mempool.space API
        Object listed = HttpRpc.get(MEMPOOL + address + "/utxo");
        if (!(listed instanceof JSONArray coins)) {
            throw new IOException("explorer's coin list is not a list");
        }
        // Initialize the list to hold the found unspent outputs
        List<UnspentOutput> found = new ArrayList<>();
        for (int index = 0; index < coins.length(); index++) {
            JSONObject coin = coins.getJSONObject(index);
            // Extract the transaction id, vout, and value for each coin
            String txid = coin.getString("txid");
            if (!txid.matches("[0-9a-f]{64}")) {
                throw new IOException("explorer's transaction id " + Text.clean(txid) + " is not 64 hex digits");
            }
            int vout = coin.getInt("vout");
            long value = coin.getLong("value");
            if (vout < 0 || value < 0 || value > Bitcoin.MAX_MONEY) {
                throw new IOException("explorer's coin " + txid + ":" + vout + " holds an impossible " + value + " sats");
            }
            // Add the unspent output to the list, including its confirmation status
            found.add(new UnspentOutput(txid, vout, value, coin.getJSONObject("status").getBoolean("confirmed")));
        }
        return found;
    }

    /// Publishes the given signed transaction to the mempool.space API
    /// Returns a confirmation message including the transaction id if successful
    static String publish(byte[] signed) throws IOException {
        String txid;
        try {
            // Post the signed transaction to the mempool.space API and get the transaction id
            txid = HttpRpc.post(BROADCAST, Text.hex(signed).substring(2)).strip();
        } catch (IOException failure) {
            throw new IOException(Text.reason(failure) + ", but the transaction may have gone out: check the address on mempool.space before signing again");
        }
        if (!txid.matches("[0-9a-f]{64}")) {
            throw new IOException("mempool.space answered " + Text.clean(txid) + " instead of a transaction id");
        }
        return ":) published, " + PUBLISHED + txid;
    }

    /// Returns the amount of satoshis held by the address according to the mempool.space stats
    private static long held(JSONObject stats) throws JSONException {
        return stats.getLong("funded_txo_sum") - stats.getLong("spent_txo_sum");
    }
}
