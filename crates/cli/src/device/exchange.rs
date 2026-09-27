use std::io::{Read, Write};
use std::time::{Duration, Instant};

use rp2350wallet_protocol::wire::{HEADER_LEN, MAX_FRAME_LEN, Request, Response, frame_len};

use crate::device::error::Error;
use crate::device::result::Result;

// Not fixed by any specification: longer than the 2 s one VTIME-bounded read can block, so
// a silent courier is reported rather than waited on.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(3);

pub fn send<W: Write>(port: &mut W, request: Request) -> Result<()> {
    let mut buffer = [0u8; MAX_FRAME_LEN];
    let written = request.encode(&mut buffer)?;

    let bytes = buffer.get(..written).ok_or(Error::FrameTooLarge)?;

    port.write_all(bytes)
        .map_err(|source| Error::WriteFailed(source.to_string()))?;
    port.flush()
        .map_err(|source| Error::WriteFailed(source.to_string()))?;

    Ok(())
}

pub fn receive<R: Read>(port: &mut R, timeout: Duration) -> Result<Response> {
    let started = Instant::now();
    let mut buffer = [0u8; MAX_FRAME_LEN];

    fill(port, &mut buffer, 0, HEADER_LEN, started, timeout)?;

    let header = buffer.get(..HEADER_LEN).ok_or(Error::FrameTooLarge)?;
    let total = frame_len(header)?;

    fill(port, &mut buffer, HEADER_LEN, total, started, timeout)?;

    let complete = buffer.get(..total).ok_or(Error::FrameTooLarge)?;

    Ok(Response::decode(complete)?)
}

fn fill<R: Read>(
    port: &mut R,
    buffer: &mut [u8],
    from: usize,
    to: usize,
    started: Instant,
    timeout: Duration,
) -> Result<()> {
    let mut filled = from;

    while filled < to {
        if started.elapsed() > timeout {
            return Err(Error::Timeout {
                expected: to,
                received: filled,
            });
        }

        let slot = buffer.get_mut(filled..to).ok_or(Error::FrameTooLarge)?;

        let read = port
            .read(slot)
            .map_err(|source| Error::ReadFailed(source.to_string()))?;

        filled += read;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::Error;
    use super::receive;
    use super::send;
    use rp2350wallet_protocol::DeviceState;
    use rp2350wallet_protocol::wire::MAX_FRAME_LEN;
    use rp2350wallet_protocol::wire::Request;
    use rp2350wallet_protocol::wire::Response;
    use rp2350wallet_protocol::wire::Slot;
    use std::io::Cursor;
    use std::time::Duration;

    const BRIEF: Duration = Duration::from_millis(50);

    fn encoded(response: Response) -> Vec<u8> {
        let mut buffer = [0u8; MAX_FRAME_LEN];
        let written = response.encode(&mut buffer).expect("encode");
        buffer[..written].to_vec()
    }

    #[test]
    fn a_request_is_written_as_a_complete_frame() {
        let mut port = Vec::new();
        send(&mut port, Request::Status).expect("send");
        assert_eq!(Request::decode(&port), Ok(Request::Status));
    }

    #[test]
    fn a_slot_selector_survives_the_trip_to_the_courier() {
        for slot in Slot::ALL {
            for request in [Request::Get(slot), Request::Clear(slot)] {
                let mut port = Vec::new();
                send(&mut port, request).expect("send");
                assert_eq!(Request::decode(&port), Ok(request));
            }
        }
    }

    #[test]
    fn a_response_is_read_back() {
        let expected = Response::Status {
            state: DeviceState::Idle.to_byte(),
            provisioned: false,
            refusal: None,
            detail: 0,
        };
        let mut port = Cursor::new(encoded(expected));
        assert_eq!(receive(&mut port, BRIEF).expect("receive"), expected);
    }

    #[test]
    fn silence_times_out_rather_than_hanging() {
        let mut port = Cursor::new(Vec::new());
        assert!(matches!(
            receive(&mut port, BRIEF),
            Err(Error::Timeout { .. })
        ));
    }

    #[test]
    fn a_truncated_response_times_out() {
        let mut bytes = encoded(Response::Status {
            state: 0xA1,
            provisioned: false,
            refusal: None,
            detail: 0,
        });
        bytes.truncate(bytes.len() - 2);
        let mut port = Cursor::new(bytes);
        assert!(matches!(
            receive(&mut port, BRIEF),
            Err(Error::Timeout { .. })
        ));
    }

    #[test]
    fn a_response_whose_opcode_is_not_ours_is_rejected_rather_than_reported() {
        let mut bytes = encoded(Response::Status {
            state: 0xA1,
            provisioned: false,
            refusal: None,
            detail: 0,
        });
        bytes[3] = 0x7F;
        let mut port = Cursor::new(bytes);
        assert!(matches!(receive(&mut port, BRIEF), Err(Error::Wire(_))));
    }

    #[test]
    fn a_response_whose_payload_does_not_match_its_opcode_is_rejected() {
        let mut bytes = encoded(Response::Status {
            state: 0xA1,
            provisioned: false,
            refusal: None,
            detail: 0,
        });
        bytes[7] = 0x02;
        let mut port = Cursor::new(bytes);
        assert!(matches!(receive(&mut port, BRIEF), Err(Error::Wire(_))));
    }

    #[test]
    fn a_response_with_a_bad_header_is_rejected_immediately() {
        let mut bytes = encoded(Response::Status {
            state: 0xA1,
            provisioned: false,
            refusal: None,
            detail: 0,
        });
        bytes[0] = 0x00;
        let mut port = Cursor::new(bytes);
        assert!(matches!(receive(&mut port, BRIEF), Err(Error::Wire(_))));
    }
}
