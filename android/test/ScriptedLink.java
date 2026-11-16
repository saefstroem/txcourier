import dev.txcourier.core.Link;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.util.Arrays;

public final class ScriptedLink implements Link {
    final ByteArrayOutputStream written = new ByteArrayOutputStream();
    private final byte[] reply;
    private int at;

    ScriptedLink(byte[] reply) {
        this.reply = reply;
    }

    @Override
    public void write(byte[] bytes) {
        written.writeBytes(bytes);
    }

    @Override
    public byte[] read(int length) throws IOException {
        if (at + length > reply.length) {
            throw new IOException("end of stream");
        }
        at += length;
        return Arrays.copyOfRange(reply, at - length, at);
    }
}
