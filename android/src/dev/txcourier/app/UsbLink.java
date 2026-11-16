package dev.txcourier.app;

import android.hardware.usb.UsbConstants;
import android.hardware.usb.UsbDevice;
import android.hardware.usb.UsbDeviceConnection;
import android.hardware.usb.UsbEndpoint;
import android.hardware.usb.UsbInterface;
import android.hardware.usb.UsbManager;
import dev.txcourier.core.Link;
import java.io.IOException;

public final class UsbLink implements Link, AutoCloseable {
    private static final int TIMEOUT_MILLIS = 3_000; // Timeout for USB operations in milliseconds
    private static final int CLASS_REQUEST_TO_INTERFACE = 0x21; // From USB to interface class request
    private static final int SET_CONTROL_LINE_STATE = 0x22; // Request number for serial standard control line state
    private static final int DTR_AND_RTS = 0x03; // Enables both Data Terminal Ready (DTR) and Request To Send (RTS) signals

    private final UsbInterface control;
    private final UsbInterface data;
    private final UsbEndpoint in;
    private final UsbEndpoint out;
    private final byte[] packet;
    private final UsbDeviceConnection connection;
    private int packetLen;
    private int packetAt;
    // Constructs a new UsbLink for the given USB device using the provided USB manager.
    public UsbLink(UsbManager manager, UsbDevice device) throws IOException {
        control = withClass(device, UsbConstants.USB_CLASS_COMM);
        data = withClass(device, UsbConstants.USB_CLASS_CDC_DATA);
        // Finds the bulk endpoints for data transfer from the data interface.
        in = bulk(data, UsbConstants.USB_DIR_IN);
        out = bulk(data, UsbConstants.USB_DIR_OUT);
        // Allocates the packet buffer based on the maximum packet size of the IN endpoint.
        packet = new byte[in.getMaxPacketSize()];
        // Opens a connection to the USB device.
        // Claims the control and data interfaces and sets the control line state for the serial port.
        connection = manager.openDevice(device);
        if (connection == null) {
            throw new IOException("courier could not be opened");
        }
        // If claiming the interfaces or setting the control line state fails, close the connection and throw an exception.
        if (!connection.claimInterface(control, true) || !connection.claimInterface(data, true)
                || connection.controlTransfer(CLASS_REQUEST_TO_INTERFACE, SET_CONTROL_LINE_STATE, DTR_AND_RTS,
                        control.getId(), null, 0, TIMEOUT_MILLIS) < 0) {
            close();
            throw new IOException("courier's serial port could not be set up");
        }
    }

    // Writes the given byte array to the USB device.
    @Override
    public void write(byte[] bytes) throws IOException {
        packetLen = 0;
        packetAt = 0;
        if (connection.bulkTransfer(out, bytes, bytes.length, TIMEOUT_MILLIS) != bytes.length) {
            throw new IOException("courier did not take the request");
        }
    }

    // Reads the specified number of bytes from the USB device.
    @Override
    public byte[] read(int length) throws IOException {
        byte[] into = new byte[length];
        // Calculate the deadline for the read operation based on the timeout.
        long deadline = System.nanoTime() + TIMEOUT_MILLIS * 1_000_000L;
        int filled = 0;
        while (filled < length) {
            if (packetAt == packetLen) {
                int left = (int) ((deadline - System.nanoTime()) / 1_000_000L);
                // Calculate the remaining time for the bulk transfer operation.
                int received = left > 0 ? connection.bulkTransfer(in, packet, packet.length, left) : -1;
                if (received < 0) {
                    throw new IOException("courier did not answer");
                }
                // Update the packet length and reset the packet read position.
                packetLen = received;
                packetAt = 0;
            }
            // Copy data from the packet buffer to the output array.
            int copied = Math.min(length - filled, packetLen - packetAt);
            System.arraycopy(packet, packetAt, into, filled, copied);
            packetAt += copied;
            filled += copied;
        }
        return into;
    }

    // Closes the USB connection and releases the claimed interfaces.
    @Override
    public void close() {
        connection.releaseInterface(data);
        connection.releaseInterface(control);
        connection.close();
    }

    // Finds and returns the USB interface of the specified class from the given USB device.
    private static UsbInterface withClass(UsbDevice device, int interfaceClass) throws IOException {
        for (int index = 0; index < device.getInterfaceCount(); index++) {
            if (device.getInterface(index).getInterfaceClass() == interfaceClass) {
                return device.getInterface(index);
            }
        }
        throw new IOException("courier is not a USB serial port");
    }

    // Finds and returns the bulk endpoint of the specified direction from the given USB interface.
    private static UsbEndpoint bulk(UsbInterface data, int direction) throws IOException {
        for (int index = 0; index < data.getEndpointCount(); index++) {
            UsbEndpoint endpoint = data.getEndpoint(index);
            if (endpoint.getType() == UsbConstants.USB_ENDPOINT_XFER_BULK && endpoint.getDirection() == direction) {
                return endpoint;
            }
        }
        throw new IOException("courier's serial port has no bulk endpoints");
    }
}
