use std::ffi::OsStr;
use std::fs::{File, OpenOptions, read_dir};
use std::os::fd::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};

use crate::device::error::Error;
use crate::device::result::Result;

// macOS mounts devfs here. The courier's CDC-ACM node appears in it.
const DEVICE_DIRECTORY: &str = "/dev";
// macOS names the CDC-ACM callout node cu.usbmodem followed by the device serial. The cu
// node opens without the carrier detect the tty node waits for.
const CALLOUT_PREFIX: &str = "cu.usbmodem";
// termios VTIME counts tenths of a second. 2 s, which must stay under REPLY_TIMEOUT so one
// read finishes inside the reply budget.
const READ_TIMEOUT_DECISECONDS: u8 = 20;

fn is_courier_port(name: &str) -> bool {
    name.starts_with(CALLOUT_PREFIX)
}

pub fn locate() -> Result<PathBuf> {
    let entries = read_dir(DEVICE_DIRECTORY)
        .map_err(|source| Error::DeviceDirectoryUnreadable(source.to_string()))?;

    let mut found = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();

        if is_courier_port(path.file_name().and_then(OsStr::to_str).unwrap_or("")) {
            found.push(path);
        }
    }

    found.sort();

    match found.as_slice() {
        [] => Err(Error::NoPortFound),
        [only] => Ok(only.clone()),
        several => Err(Error::SeveralPortsFound(
            several
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
        )),
    }
}

pub fn open(path: &Path) -> Result<File> {
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
    let mut settings = unsafe { std::mem::zeroed::<libc::termios>() };

    if unsafe { libc::tcgetattr(descriptor, &mut settings) } != 0 {
        return Err(Error::TerminalSettingsUnreadable);
    }

    unsafe { libc::cfmakeraw(&mut settings) };

    let Some(minimum) = settings.c_cc.get_mut(libc::VMIN) else {
        return Err(Error::TerminalSettingsUnwritable);
    };
    *minimum = 0;

    let Some(timeout) = settings.c_cc.get_mut(libc::VTIME) else {
        return Err(Error::TerminalSettingsUnwritable);
    };
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
