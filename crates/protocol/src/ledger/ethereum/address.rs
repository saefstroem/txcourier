use crate::ledger::error::Error;
use crate::ledger::result::Result;

pub const INSTRUCTION_ADDRESS: u8 = 0x02;

const PUBLIC_KEY_LEN: u8 = 65;
const ADDRESS_LEN: u8 = 40;

pub fn ethereum_address(reply: &[u8]) -> Result<&[u8]> {
    let [PUBLIC_KEY_LEN, rest @ ..] = reply else {
        return Err(Error::MalformedAddress);
    };
    let Some((_, [ADDRESS_LEN, address @ ..])) = rest.split_at_checked(usize::from(PUBLIC_KEY_LEN))
    else {
        return Err(Error::MalformedAddress);
    };
    if address.len() != usize::from(ADDRESS_LEN) || !address.iter().all(u8::is_ascii_hexdigit) {
        return Err(Error::MalformedAddress);
    }
    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::{Error, ethereum_address};

    #[test]
    fn only_the_ascii_address_after_a_full_public_key_is_returned() {
        let address = b"F5f107abd30e733FDB3355Fc07930410C425B7e7";
        let reply = |key: usize, length: u8, text: &[u8]| {
            [&[65][..], &vec![0x04; key], &[length], text].concat()
        };
        assert_eq!(ethereum_address(&reply(65, 40, address)), Ok(&address[..]));
        for bad in [
            reply(64, 40, address),
            reply(65, 42, address),
            reply(65, 40, &address[..39]),
            reply(65, 40, b"g5f107abd30e733FDB3355Fc07930410C425B7e7"),
            Vec::new(),
        ] {
            assert_eq!(ethereum_address(&bad), Err(Error::MalformedAddress));
        }
    }
}
