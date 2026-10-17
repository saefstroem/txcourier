package dev.txcourier.app;

import dev.txcourier.core.Courier;

/**
 * A task that can be run with a Courier instance.
 *
 * @param <T> the type of the result produced by the task
 */
public interface CourierTask<T> {
    T run(Courier courier) throws Exception;
}
