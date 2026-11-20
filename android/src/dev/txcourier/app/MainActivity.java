package dev.txcourier.app;

import android.app.Activity;
import android.app.PendingIntent;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.graphics.Insets;
import android.hardware.usb.UsbDevice;
import android.hardware.usb.UsbManager;
import android.os.Bundle;
import android.text.InputType;
import android.view.WindowInsets;
import android.widget.ArrayAdapter;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.Spinner;
import android.widget.TextView;
import dev.txcourier.core.AddressBook;
import dev.txcourier.core.Bitcoin;
import dev.txcourier.core.Chain;
import dev.txcourier.core.Courier;
import dev.txcourier.core.Node;
import dev.txcourier.core.SavedAddress;
import dev.txcourier.core.Text;
import dev.txcourier.core.Tx;
import dev.txcourier.core.UnspentOutput;
import java.io.IOException;
import java.math.BigInteger;
import java.util.Arrays;
import java.util.List;
import java.util.concurrent.Callable;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.function.Consumer;

public final class MainActivity extends Activity {
    private static final int VENDOR_ID = 0x1209; // vendor id of courier
    private static final int PRODUCT_ID = 0x0001; // product id of courier
    private static final int TEXT = InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS;
    private static final int DECIMAL = InputType.TYPE_CLASS_NUMBER | InputType.TYPE_NUMBER_FLAG_DECIMAL;
    private static final int WHOLE = InputType.TYPE_CLASS_NUMBER;
    private static final int GUTTER_DP = 16; // standard padding around the page
    private static final int BUTTON_GAP_DP = 16; // gap between buttons
    private static final int CONFIRM_GAP_DP = 64; // gap before confirmation sections
    private static final String BOOK = "addresses";
    private static final List<String> ASSETS = List.of("ETH", "BNB", "USDT", "USDC", "EURC", "PAXG", "VTIon", "IEFAon", "IEMGon", "NVDAon");

    // Executor services for handling USB and network operations.
    private final ExecutorService usbThread = Executors.newSingleThreadExecutor();
    private final ExecutorService networkThread = Executors.newSingleThreadExecutor();
    // HTTP RPC instance for communicating with the Ethereum node.
    private final HttpRpc rpc = new HttpRpc();
    // The main page layout and the currently displayed screen.
    private LinearLayout page;
    private Object screen;
    private volatile Chain handedOver;

    @Override
    protected void onCreate(Bundle saved) {
        super.onCreate(saved); // android setup
        page = new LinearLayout(this);
        page.setOrientation(LinearLayout.VERTICAL);
        page.setPadding(dp(GUTTER_DP), dp(GUTTER_DP), dp(GUTTER_DP), dp(GUTTER_DP));
        ScrollView scroll = new ScrollView(this);
        scroll.setOnApplyWindowInsetsListener((view, insets) -> {
            Insets bars = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout() | WindowInsets.Type.ime());
            view.setPadding(bars.left, bars.top, bars.right, bars.bottom);
            return WindowInsets.CONSUMED;
        });
        scroll.setFilterTouchesWhenObscured(true); // prevent touch injection
        scroll.addView(page); // add the main page layout to the scroll view
        setContentView(scroll); // set the scroll view as the content view
        menu(); // show menu
    }

    @Override
    protected void onDestroy() {
        usbThread.shutdownNow(); // shut down usb and network
        networkThread.shutdownNow();
        super.onDestroy();
    }

    private void menu() {
        blank(); // clear screen
        say("txcourier " + version()); // display the app version
        button("Show status", this::status); // show the current status
        button("EVM account", () -> account(Courier.ETHEREUM_IDENTIFIER)); // show the Ethereum account
        button("Bitcoin account", () -> account(Courier.BITCOIN_IDENTIFIER)); // show the Bitcoin account
        button("Configure addresses", this::book);
        button("Send on EVM Network", this::sendEvm);
        button("Send bitcoin", this::sendBitcoin);
        button("Show the signed transaction", this::signed); // display the signed transaction
        button("Clear the pending transaction", () -> discard(Courier.PENDING, "pending")); // clear the pending transaction
        button("Clear the signed transaction", () -> discard(Courier.SIGNED, "signed")); // clear the signed transaction
    }

    private void status() {
        open(); // call open to prepare the screen for displaying status
        courier(Courier::status, this::lines); // fetch and display the current status
    }

    // Shows an account based on the users requested account type
    private void account(byte account) {
        open(); // opens main screen
        boolean evm = account == Courier.ETHEREUM_IDENTIFIER; // detect whihc account user wants to see
        List<SavedAddress> book = AddressBook.read(stored());
        List<SavedAddress> saved = evm ? AddressBook.evm(book) : AddressBook.bitcoin(book);
        if (saved.isEmpty()) {
            say("add a " + (evm ? "0x" : "bc1q") + " address under Configure addresses first");
            return;
        }
        Spinner chosen = spinner(saved);
        if (evm) { // if the account is an EVM account, show balances for each known chain
            for (Chain chain : Chain.KNOWN) { // for each known EVM chain, provide a button to check balances
                button(chain.name() + " balances", () -> {
                    String address = ((SavedAddress) chosen.getSelectedItem()).address();
                    open();
                    say(chain + " " + address);
                    run(networkThread, () -> Node.balances(rpc, chain, address), this::lines); // read the balances then print lines with it
                });
            }
        } else {
            button("Bitcoin balance", () -> { // read the ledgers bitcoin balance
                String address = ((SavedAddress) chosen.getSelectedItem()).address();
                open();
                say("Bitcoin " + address);
                run(networkThread, () -> BitcoinExplorer.balance(address), this::say); // get the balance and state it
            });
        }
    }

    // shows the address book view
    private void book() {
        open(); // clear the view
        button("+", () -> { // add a new address
            open();
            EditText label = field("Label", TEXT);
            EditText address = field("Address (0x... or bc1q...)", TEXT);
            button("Save", () -> {
                save(AddressBook.added(stored(), text(label), text(address))); // save the new address
                book(); // refresh the address book view
            });
        });
        List<SavedAddress> saved = AddressBook.read(stored());
        if (saved.isEmpty()) {
            say("no addresses yet, add one with +");
        }
        for (SavedAddress entry : saved) {
            say(entry.toString());
            hold("Hold to remove " + entry.label(), () -> { // remove an address on hold
                save(AddressBook.removed(stored(), entry)); // save the updated address book
                book(); // refresh the address book view
            });
        }
    }

    // retrieves the stored address book string
    private String stored() {
        return getSharedPreferences(BOOK, MODE_PRIVATE).getString(BOOK, "");
    }

    // saves the updated address book string
    private void save(String stored) {
        getSharedPreferences(BOOK, MODE_PRIVATE).edit().putString(BOOK, stored).commit();
    }

    // initiates the process of sending Bitcoin
    private void sendBitcoin() {
        open(); // clear the view for sending Bitcoin
        // get the list of saved Bitcoin addresses
        List<SavedAddress> saved = AddressBook.bitcoin(AddressBook.read(stored()));
        if (saved.isEmpty()) {
            say("add a bc1q address under Configure addresses first");
            return;
        }
        // display the sender selection UI
        say("Sender");
        Spinner from = spinner(saved);
        // let the user select the sender address from the saved Bitcoin addresses
        button("Show its coins", () -> sendBitcoin(((SavedAddress) from.getSelectedItem()).address(), saved));
    }

    // handles the process of sending Bitcoin after the sender is selected
    private void sendBitcoin(String address, List<SavedAddress> saved) {
        open();
        say("coins of " + address); // display the coins available at the address
        run(networkThread, () -> BitcoinExplorer.coins(address), coins -> { // fetch the UTXOs at the address
            if (coins.isEmpty()) {
                say("this address has no coins to spend"); // inform the user that there are no UTXOs
                return;
            }
            Spinner coin = spinner(coins); // let the user select a UTXO to spend
            say("Recipient");
            Spinner to = spinner(saved); // let the user select the recipient address from the saved Bitcoin addresses
            EditText amount = field("Amount in BTC (leave empty to send the whole coin)", DECIMAL); // input for the amount to send
            EditText rate = field("Fee rate in sat/vB", DECIMAL); // input for the fee rate
            button("Review", () -> { // button to review the transaction before sending
                byte[] record = Bitcoin.build(address, (UnspentOutput) coin.getSelectedItem(), ((SavedAddress) to.getSelectedItem()).address(), text(amount), text(rate)); // build the tx
                open();
                lines(Bitcoin.describe(record)); // describe the transaction for review
                confirm("Hand this transaction to the courier?", () -> courier(courier -> { // send the transaction to the courier for signing
                    courier.putBitcoin(record); // hand the built transaction to the courier for signing
                    return ":) courier is holding the transaction, plug in the Ledger and open its Bitcoin app to sign it."
                            + " Compare the recipient address it shows with yours, to the last character"; // inform the user to sign the tx with the Ledger
                }, this::say));
            });
        });
    }

    private void sendEvm() {
        open(); // open the screen for sending a new transaction
        // get the list of saved EVM addresses
        List<SavedAddress> saved = AddressBook.evm(AddressBook.read(stored()));
        if (saved.isEmpty()) {
            say("add a 0x address under Configure addresses first");
            return;
        }
        say("Sender"); // select the sender address for the EVM transaction
        Spinner from = spinner(saved);
        Spinner chains = spinner(Chain.KNOWN);
        Spinner asset = spinner(ASSETS);
        say("Recipient"); // select the recipient address for the EVM transaction
        Spinner to = spinner(saved);
        EditText amount = field("Amount (whole coin units)", DECIMAL);
        EditText nonce = field("Nonce (filled in by Fetch nonce and fees)", WHOLE);
        EditText tip = field("Priority fee in gwei", DECIMAL);
        EditText maxFee = field("Max fee in gwei", DECIMAL);
        EditText gas = field("Gas limit (leave empty for default)", WHOLE);
        // button to suggest appropriate fees based on the selected asset's chain
        button("Fetch nonce and fees", () -> {
            // determine the chain of the selected asset
            Chain chain = (Chain) chains.getSelectedItem();
            Object sender = from.getSelectedItem(); // get the sender
            String address = ((SavedAddress) sender).address(); // cast and get the address
            run(networkThread, () -> {
                BigInteger[] fees = Node.suggest(rpc, chain);
                return new BigInteger[] {fees[0], fees[1], Node.nonce(rpc, chain, address)};
            }, fees -> {
                // if anyone changed the selected asset's chain while fees were being suggested
                if (chains.getSelectedItem() != chain || from.getSelectedItem() != sender) {
                    // the selected asset's chain has changed, so we should not use the suggested fees
                    say("x another chain or sender is chosen now, fetch nonce and fees again");
                    return;
                }
                // ensure the priority fee is raised by 50% for faster inclusion
                BigInteger raised = Node.raise(fees[1]);
                say(chain + ": base fee " + gwei(fees[0]) + " gwei, priority fee " + gwei(fees[1])
                        + " gwei, suggested with 50% more priority fee for faster inclusion");
                nonce.setText(fees[2].toString());
                tip.setText(gwei(raised));
                maxFee.setText(gwei(Node.maxFee(fees[0], raised)));
            });
        });
        // button to review the transaction before sending
        button("Review", () -> {
            Chain chain = (Chain) chains.getSelectedItem();
            String typed = text(nonce);
            String address = ((SavedAddress) from.getSelectedItem()).address(); // get the sender's address for the EVM transaction
            byte[] record = Tx.build(chain, (String) asset.getSelectedItem(), ((SavedAddress) to.getSelectedItem()).address(), text(amount), typed, text(tip),
                    text(maxFee), text(gas));
            open();
            say("checking the nonce...");
            run(networkThread, () -> Node.checkNonce(rpc, chain, address, typed), next -> reviewEvm(record, chain));
        });
    }

    // Reviews the unsigned transaction and allows the user to hand it to the courier.
    private void reviewEvm(byte[] record, Chain chain) {
        // describe the unsigned transaction for the user to review
        List<String> lines = Tx.describe(record);
        open();
        // show the transaction details to the user for review
        lines(lines);
        // ask the user to confirm handing the transaction to the courier
        confirm("Hand this transaction to the courier?", () -> courier(courier -> {
            courier.putEVM(record);
            handedOver = chain;
            return ":) courier is holding the transaction, plug in the Ledger to sign it";
        }, this::say));
    }

    private void signed() {
        open();
        // Get the slot data and display it to user
        courier(courier -> courier.get(Courier.SIGNED), bytes -> {
            if (bytes.length == 0) {
                say("there is no signed transaction");
                return;
            }
            byte[] signed = Arrays.copyOfRange(bytes, 1, bytes.length);
            say(Text.hex(signed));
            if (bytes[0] == Courier.BITCOIN_IDENTIFIER) { // check if the transaction is signed for Bitcoin
                confirm("Publish it on Bitcoin? This cannot be undone.",
                        () -> run(networkThread, () -> BitcoinExplorer.publish(signed), this::say));
                return;
            }
            if (bytes[0] != Courier.ETHEREUM_IDENTIFIER) {
                say("x courier's signed transaction has no Ethereum or Bitcoin identifier");
                return;
            }
            Chain chain = handedOver;
            if (chain != null) {
                confirm("Publish it on " + chain + "? This cannot be undone.", () -> publish(chain, signed));
                return;
            }
            say("this app has not handed a transaction to the courier since it started, so it does not know the chain."
                    + " Choose the chain the transaction was made for. Its nodes refuse it if the choice is wrong.");
            button("No", this::menu);
            for (Chain known : Chain.KNOWN) {
                hold("Hold to publish on " + known, () -> publish(known, signed));
            }
        });
    }

    // Publishes a signed transaction to the network.
    private void publish(Chain chain, byte[] signed) {
        // inform the user that the publishing process has started
        say("publishing...");
        Object started = screen;
        // run the network operation to publish the transaction
        run(networkThread, () -> Node.publish(rpc, chain, signed,
                (url, failure) -> later(started, () -> say("x " + url + ": " + Text.reason(failure)))), this::say);
    }

    // Discards a transaction from a specific slot in the courier.
    private void discard(int slot, String name) {
        // Open the courier interface and ask for confirmation before discarding the transaction
        open();
        confirm("Discard the " + name + " transaction?", () -> courier(courier -> {
            courier.clear(slot);
            return ":) " + name + " slot is now empty";
        }, this::say));
    }

    // Executes a task with the courier, handling USB connection and permissions.
    private <T> void courier(CourierTask<T> task, Consumer<T> then) {
        run(usbThread, () -> {
            // get usb
            UsbManager usb = getSystemService(UsbManager.class);

            // retrieve usb device
            List<UsbDevice> found = usb.getDeviceList().values().stream()
                    .filter(device -> device.getVendorId() == VENDOR_ID && device.getProductId() == PRODUCT_ID).toList();
            if (found.size() != 1) {
                throw new IOException(found.isEmpty()
                        ? "no courier found, is it plugged in and running the firmware?"
                        : "several couriers are connected, leave only one plugged in");
            }
            // select the first (and only) found device
            UsbDevice device = found.get(0);
            if (!usb.hasPermission(device)) {
                Intent answer = new Intent("dev.txcourier.USB_PERMISSION").setPackage(getPackageName());
                usb.requestPermission(device, PendingIntent.getBroadcast(this, 0, answer, PendingIntent.FLAG_MUTABLE));
                throw new IOException("allow txcourier to use the courier, then try again");
            }
            // open a link to the usb device and run the courier task
            try (UsbLink link = new UsbLink(usb, device)) {
                return task.run(new Courier(link));
            }
        }, then);
    }

    // Executes a task on a specific thread and handles the result on the UI thread.
    private <T> void run(ExecutorService thread, Callable<T> work, Consumer<T> then) {
        Object started = screen;
        thread.execute(() -> {
            try {
                T result = work.call();
                later(started, () -> then.accept(result));
            } catch (Throwable failure) {
                later(started, () -> say("x " + Text.reason(failure)));
            }
        });
    }

    // Schedules a step to run on the UI thread after verifying that the screen has not changed.
    private void later(Object started, Runnable step) {
        runOnUiThread(() -> {
            if (started == screen) { // only if we are still on the same screen
                guarded(step); //failsafe run
            }
        });
    }

    // Executes a step with a failsafe, catching any runtime exceptions and displaying an error message.
    private void guarded(Runnable step) {
        try {
            step.run();
        } catch (RuntimeException failure) {
            say("x " + Text.reason(failure));
        }
    }

    // Clears the current screen and prepares a blank page.
    private void blank() {
        screen = new Object();
        page.removeAllViews();
    }

    // Opens the main screen with the menu button.
    private void open() {
        blank();
        button("Menu", this::menu);
    }

    // Displays a message to the user in a new TextView.
    private TextView say(String line) {
        TextView view = new TextView(this);
        view.setText(line);
        view.setTextIsSelectable(true);
        page.addView(view);
        return view;
    }

    // Displays multiple lines of text to the user, each in a new TextView.
    private void lines(List<String> lines) {
        lines.forEach(this::say);
    }

    // Creates a button with the given label and action, adding it to the current page.
    private Button button(String label, Runnable action) {
        Button button = new Button(this);
        button.setText(label);
        button.setOnClickListener(clicked -> guarded(action));
        page.addView(button, spaced(BUTTON_GAP_DP));
        return button;
    }

    // Creates layout parameters with a top margin for spacing between UI elements.
    private LinearLayout.LayoutParams spaced(int gapDp) {
        LinearLayout.LayoutParams spacing = new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT);
        spacing.topMargin = dp(gapDp);
        return spacing;
    }

    // Creates an EditText field with a label and specified input type, adding it to the current page.
    private EditText field(String label, int inputType) {
        say(label);
        EditText field = new EditText(this);
        field.setInputType(inputType);
        field.setSingleLine(true);
        page.addView(field);
        return field;
    }

    // Creates a Spinner (dropdown) with the given choices, adding it to the current page.
    private <T> Spinner spinner(List<T> choices) {
        Spinner spinner = new Spinner(this);
        spinner.setAdapter(new ArrayAdapter<>(this, android.R.layout.simple_spinner_dropdown_item, choices));
        page.addView(spinner);
        return spinner;
    }

    // Displays a confirmation prompt with "No" and "Hold to confirm" options.
    private void confirm(String question, Runnable yes) {
        say(question);
        button("No", this::menu);
        hold("Hold to confirm", yes);
    }

    private void hold(String label, Runnable action) {
        Button hold = button(label, () -> say("hold the button down to confirm"));
        hold.setLayoutParams(spaced(CONFIRM_GAP_DP));
        hold.setOnLongClickListener(held -> {
            guarded(action);
            return true;
        });
    }

    // Converts a value in density-independent pixels (dp) to pixels (px) based on the device's screen density.
    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }

    // Retrieves the version name of the application from the package manager.
    private String version() {
        try {
            return getPackageManager().getPackageInfo(getPackageName(), PackageManager.PackageInfoFlags.of(0)).versionName;
        } catch (PackageManager.NameNotFoundException impossible) {
            return "of unknown version";
        }
    }

    // Retrieves the text content from an EditText field as a String.
    private static String text(EditText field) {
        return field.getText().toString();
    }

    // Converts a value in wei to gwei and returns it as a formatted string.
    private static String gwei(BigInteger wei) {
        return Text.amount(wei, 9);
    }
}
