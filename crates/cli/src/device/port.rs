use std::ffi::OsStr;
use std::fs::{File, OpenOptions, read_dir};
use std::os::fd::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};

use crate::device::error::Error;
use crate::device::result::Result;

// mounts devfs here.
const DEVICE_DIRECTORY: &str = "/dev";
// macOS names the CDC-ACM callout node cu.usbmodem followed by the device serial.
const CALLOUT_PREFIX: &str = "cu.usbmodem";
// termios VTIME counts tenths of a second. 2 s, which must stay under REPLY_TIMEOUT
const READ_TIMEOUT_DECISECONDS: u8 = 20;

/// Optimistically determine this is the tx courier
fn is_courier_port(name: &str) -> bool {
    name.starts_with(CALLOUT_PREFIX)
}

/// Compute device candidates for where the RP2350 is connected
pub fn candidates() -> Result<Vec<PathBuf>> {
    let entries = read_dir(DEVICE_DIRECTORY)
        .map_err(|source| Error::DeviceDirectoryUnreadable(source.to_string()))?;

    let mut found = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();

        // Check if its the port of the rp2350
        if is_courier_port(path.file_name().and_then(OsStr::to_str).unwrap_or("")) {
            found.push(path);
        }
    }

    found.sort();

    Ok(found)
}

pub fn open(path: &Path) -> Result<File> {
    // open the file to the tx courier
    let port = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|source| Error::PortUnopenable {
            path: path.display().to_string(),
            reason: source.to_string(),
        })?;

    make_raw(port.as_raw_fd())?;

    Ok(port)
}

fn make_raw(descriptor: RawFd) -> Result<()> {
    // create new settings file
    let mut settings = unsafe { std::mem::zeroed::<libc::termios>() };

    // read the ports current settings to the settings struct
    if unsafe { libc::tcgetattr(descriptor, &mut settings) } != 0 {
        return Err(Error::TerminalSettingsUnreadable);
    }

    // remove all kinds of default processing
    unsafe { libc::cfmakeraw(&mut settings) };

    // compute minimum of bytes read before it returns
    let Some(minimum) = settings.c_cc.get_mut(libc::VMIN) else {
        return Err(Error::TerminalSettingsUnwritable);
    };

    // set it to 0 which means we read as soon as we get any bytes
    *minimum = 0;

    // read the timeout configuration
    let Some(timeout) = settings.c_cc.get_mut(libc::VTIME) else {
        return Err(Error::TerminalSettingsUnwritable);
    };
    // timeout when we are trying to read
    *timeout = READ_TIMEOUT_DECISECONDS;

    if unsafe { libc::tcsetattr(descriptor, libc::TCSANOW, &settings) } != 0 {
        return Err(Error::TerminalSettingsUnwritable);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_courier_port;
    use super::open;
    use std::path::Path;

    #[test]
    fn callout_devices_are_recognised() {
        assert!(is_courier_port("cu.usbmodem0001"));
        assert!(is_courier_port("cu.usbmodem14201"));
    }

    #[test]
    fn the_dial_in_device_of_the_same_port_is_ignored() {
        assert!(!is_courier_port("tty.usbmodem0001"));
    }

    #[test]
    fn unrelated_devices_are_ignored() {
        assert!(!is_courier_port("cu.Bluetooth-Incoming-Port"));
        assert!(!is_courier_port("cu.debug-console"));
        assert!(!is_courier_port("disk0"));
        assert!(!is_courier_port(""));
    }

    #[test]
    fn a_missing_port_reports_the_path_it_tried() {
        let path = Path::new("/dev/cu.usbmodem-does-not-exist");
        let message = match open(path) {
            Ok(_) => panic!("a nonexistent device should not open"),
            Err(error) => error.to_string(),
        };
        assert!(message.contains("cu.usbmodem-does-not-exist"));
    }
}
