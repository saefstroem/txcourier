package dev.txcourier.core;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

public final class AddressBook {
    private AddressBook() {
    }

    // Reads the stored address book into a list of SavedAddress objects
    public static List<SavedAddress> read(String stored) {
        // Split the stored string by newlines and parse each line into a SavedAddress object
        List<SavedAddress> book = new ArrayList<>();
        // Iterate over each line in the stored string
        for (String line : stored.split("\n")) {
            int space = line.indexOf(' ');
            // Find the first space to separate the address from the label
            if (space > 0) {
                // Create a new SavedAddress with the label and address and add it to the book
                book.add(new SavedAddress(line.substring(space + 1), line.substring(0, space)));
            }
        }
        return book;
    }

    // Adds a new address to the stored address book string
    public static String added(String stored, String label, String typed) {
        // Clean and validate the label, normalize the address, and check for duplicates
        String name = Text.clean(label).strip();
        if (name.isEmpty()) {
            throw new IllegalArgumentException("a label is needed");
        }
        String address = normalized(typed);
        for (SavedAddress saved : read(stored)) {
            if (saved.address().equals(address)) {
                throw new IllegalArgumentException("this address is already saved as " + saved.label());
            }
        }
        // Append the new address and label to the stored string and return it
        return stored + address + " " + name + "\n";
    }

    // Removes an address from the stored address book string
    public static String removed(String stored, SavedAddress gone) {
        // Iterate over the stored addresses and keep only those that are not equal to the one being removed
        StringBuilder kept = new StringBuilder();
        // Initialize a StringBuilder to collect the addresses that will be kept
        for (SavedAddress saved : read(stored)) {
            // Check if the current saved address is the one to be removed
            if (!saved.equals(gone)) {
                // If the current saved address is not the one to be removed, add it to the kept list
                kept.append(saved.address()).append(' ').append(saved.label()).append('\n');
            }
        }
        return kept.toString();
    }

    // Filters the address book to include only EVM addresses (those starting with "0x")
    public static List<SavedAddress> evm(List<SavedAddress> book) {
        return book.stream().filter(saved -> saved.address().startsWith("0x")).toList();
    }

    // Filters the address book to include only Bitcoin addresses
    public static List<SavedAddress> bitcoin(List<SavedAddress> book) {
        return book.stream().filter(saved -> saved.address().matches(Bitcoin.ADDRESS)).toList();
    }

    // Filters the address book to include only addresses that are EVM or Bitcoin
    private static String normalized(String typed) {
        String address = typed.strip().toLowerCase(Locale.ROOT);
        if (address.matches(Bitcoin.ADDRESS)) {
            return address;
        }
        if (address.matches("(0x)?[0-9a-f]{40}")) {
            return Text.hex(Text.address(address));
        }
        throw new IllegalArgumentException("'" + Text.clean(typed) + "' is neither 0x and 40 hex digits nor a 42-character bc1q address");
    }
}
