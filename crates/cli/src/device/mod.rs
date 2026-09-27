mod error;
mod exchange;
mod port;
mod result;

pub use error::Error;
pub use port::locate;
pub use result::Result;

use std::path::Path;

use rp2350wallet_protocol::wire::{Request, Response};

pub fn ask(path: &Path, request: Request) -> Result<Response> {
    let mut courier = port::open(path)?;
    exchange::send(&mut courier, request)?;
    exchange::receive(&mut courier, exchange::REPLY_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rp2350wallet_protocol::wire::Request;

    use super::ask;

    const MISSING_DEVICE: &str = "/dev/cu.usbmodem-does-not-exist";

    #[test]
    fn asking_a_missing_device_fails_before_anything_is_sent() {
        assert!(ask(Path::new(MISSING_DEVICE), Request::Status).is_err());
    }
}
