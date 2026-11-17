use crate::bitcoin::{Draft, PUBLIC_KEY_LEN, SIGNATURE_MAX, put, signed};
use crate::ledger::error::Error;
use crate::ledger::result::Result;
use sha2::{Digest, Sha256};

pub const FINGERPRINT: [u8; 4] = [0xE1, 0x05, 0x00, 0x01]; // GET_MASTER_FINGERPRINT, CLA E1 INS 05 P1 00 reserved P2 protocol version 1: no data in, the master key's 4-byte fingerprint out, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L11 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L13 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L21 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L284-L290 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/src/constants.h#L23-L25 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/src/boilerplate/dispatcher.c#L135 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0032.mediawiki?plain=1#L126
pub const EXTENDED_PUBKEY: [u8; 4] = [0xE1, 0x00, 0x00, 0x01]; // GET_EXTENDED_PUBKEY, CLA E1 INS 00 P1 00 reserved P2 protocol version 1: a display flag and a path in, the xpub as base58 text out, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L11 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L17 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L88-L99 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/src/handler/get_extended_pubkey.c#L160-L170 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/src/constants.h#L25
pub const SIGN_PSBT: [u8; 4] = [0xE1, 0x04, 0x00, 0x01]; // SIGN_PSBT, CLA E1 INS 04 P1 00 reserved P2 protocol version 1, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L11 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L20 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L214-L222 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/src/constants.h#L25
pub const CONTINUE: [u8; 4] = [0xF8, 0x01, 0x00, 0x01]; // CONTINUE, framework CLA F8 INS 01 P1 00 reserved P2 protocol version 1: carries the answer to a client command, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L11 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L24-L30 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/src/constants.h#L25
pub const STATUS_INTERRUPTED: [u8; 2] = [0xE0, 0x00]; // SW_INTERRUPTED_EXECUTION, the status word the Ledger returns when it needs an answer before it can continue, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L34 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L65 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/src/boilerplate/sw.h#L90
// GET_EXTENDED_PUBKEY data for the account key m/84'/0'/0': display 0, 3 steps, each a big-endian u32 with the hardened bit 0x80000000, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L88-L93 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0084.mediawiki?plain=1#L73 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0032.mediawiki?plain=1#L75
pub const ACCOUNT_REQUEST: [u8; 14] = [
    0x00, 0x03, 0x80, 0x00, 0x00, 0x54, 0x80, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00,
];
// GET_EXTENDED_PUBKEY data for the signing key m/84'/0'/0'/0/0, the first receive address, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L88-L93 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0084.mediawiki?plain=1#L77 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0032.mediawiki?plain=1#L75
pub const KEY_REQUEST: [u8; 22] = [
    0x00, 0x05, 0x80, 0x00, 0x00, 0x54, 0x80, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

const DIGEST_LEN: usize = 32; // SHA-256 output, the hash behind every Merkle leaf and node and the wallet policy id, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/merkle.md?plain=1#L14 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/merkle.md?plain=1#L22 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/wallet.md?plain=1#L82

type Hash = [u8; DIGEST_LEN]; // one SHA-256 digest, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/merkle.md?plain=1#L14

const TEMPLATE: &[u8] = b"wpkh(@0/**)"; // descriptor template of the default native segwit wallet: key 0 on the receive and change branches, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/wallet.md?plain=1#L41-L51 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0388.mediawiki?plain=1#L164
const ORIGIN: &[u8] = b"/84'/0'/0']"; // key origin after the fingerprint: the BIP-84 account path, then the closing bracket; required for a key the Ledger holds, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0388.mediawiki?plain=1#L182-L186 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0084.mediawiki?plain=1#L73 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/wallet.md?plain=1#L13-L14 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/wallet.md?plain=1#L54-L55
const XPUB_LEN: usize = 111; // a mainnet xpub is 111 base58 characters, 78 bytes and a 4-byte checksum under version 0488B21E; the Ledger sends it as text and allows 111 or 112, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0032.mediawiki?plain=1#L131-L138 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0032.mediawiki?plain=1#L222 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/src/handler/get_extended_pubkey.c#L160-L170
const KEY_LEN: usize = 9 + ORIGIN.len() + XPUB_LEN; // one key information string: "[" and 8 fingerprint hex digits, then ORIGIN with its closing "]", then the xpub, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0388.mediawiki?plain=1#L182-L187
const POLICY_LEN: usize = 3 + DIGEST_LEN + 1 + DIGEST_LEN; // serialized default wallet policy: version 02, name length 00 and no name, template length 0b, sha256 of the template, key count 01, Merkle root of the one key; both varints are one byte below 253, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/wallet.md?plain=1#L68-L78 https://developer.bitcoin.org/reference/transactions.html#compactsize-unsigned-integers
const COMMITMENT_LEN: usize = 1 + 2 * DIGEST_LEN; // Merkleized map commitment: pair count as a one-byte varint, keys root, values root, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/merkle.md?plain=1#L64-L69
const SIGN_REQUEST_LEN: usize = COMMITMENT_LEN + 2 * (1 + DIGEST_LEN) + 2 * DIGEST_LEN; // SIGN_PSBT data: global map commitment, input count and root, output count and root, wallet id, 32 zero bytes of hmac, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L214-L222 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L234
const BASE58: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"; // bitcoin's base58 alphabet, see https://github.com/bitcoin/bitcoin/blob/v32.1/src/base58.cpp#L20
const KEY_AT: usize = 45; // offset of the 33-byte key in a decoded xpub: version 4, depth 1, parent fingerprint 4, child number 4, chain code 32, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0032.mediawiki?plain=1#L131-L136
const XPUB_CHECKSUM_LEN: usize = 4; // checksum at the end of a decoded xpub, the first 4 bytes of a double SHA-256 of the 78 before it, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0032.mediawiki?plain=1#L138
// the signing key's path m/84'/0'/0'/0/0 as the PSBT stores it: 32-bit little-endian steps with the hardened bit 0x80000000, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0174.mediawiki?plain=1#L215-L216 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0084.mediawiki?plain=1#L77 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0032.mediawiki?plain=1#L75
const PATH: [[u8; 4]; 5] = [
    0x8000_0054u32.to_le_bytes(),
    0x8000_0000u32.to_le_bytes(),
    0x8000_0000u32.to_le_bytes(),
    0u32.to_le_bytes(),
    0u32.to_le_bytes(),
];

const GLOBAL_TX_VERSION: u8 = 0x02; // PSBT_GLOBAL_TX_VERSION, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0370.mediawiki?plain=1#L59-L64
const IN_WITNESS_UTXO: u8 = 0x01; // PSBT_IN_WITNESS_UTXO: the spent output, amount then script, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0174.mediawiki?plain=1#L160-L166
const IN_BIP32_DERIVATION: u8 = 0x06; // PSBT_IN_BIP32_DERIVATION: key byte and pubkey, value fingerprint and path, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0174.mediawiki?plain=1#L210-L216
const IN_PREVIOUS_TXID: u8 = 0x0E; // PSBT_IN_PREVIOUS_TXID, the txid in internal byte order, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0370.mediawiki?plain=1#L123-L128
const IN_OUTPUT_INDEX: u8 = 0x0F; // PSBT_IN_OUTPUT_INDEX, 32-bit little-endian, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0370.mediawiki?plain=1#L133-L138
const OUT_AMOUNT: u8 = 0x03; // PSBT_OUT_AMOUNT, 64-bit little-endian, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0370.mediawiki?plain=1#L187-L192
const OUT_SCRIPT: u8 = 0x04; // PSBT_OUT_SCRIPT, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0370.mediawiki?plain=1#L197-L202
const TX_VERSION: u32 = 2; // value of PSBT_GLOBAL_TX_VERSION, 32-bit little-endian; the signature covers it, so it must equal VERSION in bitcoin/transaction.rs, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0370.mediawiki?plain=1#L59-L64 https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0143.mediawiki?plain=1#L29

const YIELD: u8 = 0x10; // client command YIELD: the Ledger hands over input 0's signature as index, key length, key, signature, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L356 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L364 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L242-L246
const GET_PREIMAGE: u8 = 0x40; // client command GET_PREIMAGE: 00 and a sha256 in; preimage length, included length and bytes out, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L357 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L372-L383
const GET_MERKLE_LEAF_PROOF: u8 = 0x41; // client command GET_MERKLE_LEAF_PROOF: root, tree size, index in; leaf hash, proof length, included count, hashes out, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L358 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L389-L402
const GET_MERKLE_LEAF_INDEX: u8 = 0x42; // client command GET_MERKLE_LEAF_INDEX: root and leaf hash in; found flag and index out, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L359 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L408-L418

const MAX_PROOF: usize = 2; // deepest Merkle proof here: the input map has 4 keys, so a proof holds 2 sibling hashes, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/merkle.md?plain=1#L36-L40
const MAX_LEAVES: usize = 4; // most leaves in one tree: the input map's 4 keys or 4 values; txcourier's own count
const ELEMENT_MAX: usize = KEY_LEN; // the largest element, the key information string; a commitment is 65 bytes and the derivation key 34; txcourier's own

// every Merkle tree the Ledger can ask about: three plain lists (the wallet policy's key list and the lists of input and output map commitments) and a keys tree and a values tree for each of the four PSBT maps, see https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/wallet.md?plain=1#L76-L78 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/bitcoin.md?plain=1#L214-L220 https://github.com/LedgerHQ/app-bitcoin/blob/44ecd0410dcd/doc/merkle.md?plain=1#L54-L60
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tree {
    Wallet,
    GlobalKeys,
    GlobalValues,
    InputKeys,
    InputValues,
    RecipientKeys,
    RecipientValues,
    ChangeKeys,
    ChangeValues,
    Inputs,
    Outputs,
}

const TREES: [Tree; 11] = [
    Tree::Wallet,
    Tree::GlobalKeys,
    Tree::GlobalValues,
    Tree::InputKeys,
    Tree::InputValues,
    Tree::RecipientKeys,
    Tree::RecipientValues,
    Tree::ChangeKeys,
    Tree::ChangeValues,
    Tree::Inputs,
    Tree::Outputs,
];

pub struct Client {
    signing: Option<(Draft, [u8; PUBLIC_KEY_LEN])>,
    fingerprint: [u8; 4],
    key: [u8; KEY_LEN],
    policy: [u8; POLICY_LEN],
    id: Hash,
    signature: [u8; SIGNATURE_MAX],
    signature_len: usize,
}

impl Client {
    pub fn new(
        fingerprint: &[u8],
        xpub: &[u8],
        signing: Option<(Draft, [u8; PUBLIC_KEY_LEN])>,
    ) -> Result<Self> {
        // validate the fingerprint and xpub length
        let (Ok(fingerprint), XPUB_LEN) = (<[u8; 4]>::try_from(fingerprint), xpub.len()) else {
            return Err(Error::MalformedKey);
        };
        let digit = |nibble: u8| char::from_digit(nibble.into(), 16).map_or(b'0', |hex| hex as u8);
        // convert the fingerprint to hexadecimal digits
        let digits = fingerprint.map(|byte| [digit(byte >> 4), digit(byte & 0x0F)]);
        let mut key = [0u8; KEY_LEN];
        // fill the key with the fingerprint, origin, and xpub
        fill(&mut key, &[b"[", digits.as_flattened(), ORIGIN, xpub])?;
        // compute the template and wallet digests
        let (template, wallet) = (digest(&[TEMPLATE]), digest(&[&[0x00], &key]));
        let header = [0x02, 0x00, TEMPLATE.len() as u8];
        let mut policy = [0u8; POLICY_LEN];
        // fill the policy with the header, template, and wallet digests
        fill(&mut policy, &[&header, &template, &[1], &wallet])?;
        Ok(Self {
            signing,
            fingerprint,
            key,
            policy,
            id: digest(&[&policy]),
            signature: [0; SIGNATURE_MAX],
            signature_len: 0,
        })
    }

    /// Creates a sign request for the client, including the global commitment, input and output roots, and client ID.
    pub fn sign_request(&self) -> Result<[u8; SIGN_REQUEST_LEN]> {
        // Compute the global commitment for the transaction
        let global = self.commitment(Tree::GlobalKeys, Tree::GlobalValues)?;
        // Compute the roots for the inputs and outputs
        let (inputs, outputs) = (self.root_of(Tree::Inputs)?, self.root_of(Tree::Outputs)?);
        // Compute the count of output leaves
        let count = [self.count(Tree::Outputs) as u8];
        // Initialize the request buffer for the sign request
        let mut request = [0u8; SIGN_REQUEST_LEN];
        // Fill the request buffer with the global commitment, input and output roots, output count, and client ID
        fill(
            &mut request,
            &[&global, &[1], &inputs, &count, &outputs, &self.id],
        )?;
        Ok(request)
    }

    /// Handles an incoming command from the client and produces the appropriate response.
    pub fn answer(&mut self, command: &[u8], out: &mut [u8]) -> Result<usize> {
        match command {
            [YIELD, yielded @ ..] => self.take(yielded), // the ledger gave us a signature so we store it in the client
            [GET_PREIMAGE, 0, hash @ ..] => {
                // Retrieves the preimage of a hash based on the hash value provided
                let mut element = [0u8; ELEMENT_MAX];
                let (prefix, preimage): (&[u8], &[u8]) = if hash == self.id {
                    // If the hash matches the client ID, return the policy as the preimage
                    (&[], &self.policy)
                } else if hash == digest(&[TEMPLATE]) {
                    // If the hash matches the template digest, return the template as the preimage 
                    (&[], TEMPLATE)
                } else {
                    // Otherwise, locate the element in the Merkle tree corresponding to the hash and return it as the preimage
                    let (tree, index) = self.locate(hash)?;
                    let len = self.element(tree, index, &mut element)?;
                    (&[0x00], element.get(..len).unwrap_or(&[]))
                };
                // Compute the total length of the prefix and preimage and fill the output buffer with it.
                let len = [(prefix.len() + preimage.len()) as u8];
                // Fill the output buffer with the length, prefix, and preimage.
                fill(out, &[&len, &len, prefix, preimage])
            }
            [GET_MERKLE_LEAF_PROOF, query @ ..] => {
                // Get the top of the Merkle tree and the query parameters (size and index) from the client command.
                let Some((top, &[size, index])) = query.split_first_chunk::<DIGEST_LEN>() else {
                    return Err(Error::UnknownCommitment);
                };
                // Find the Merkle tree corresponding to the top hash and the specified size.
                let tree = self.find(top, Some(size))?;
                // Retrieve the leaves of the Merkle tree and prepare the proof path.
                // Initialize an array to hold the leaves of the Merkle tree.
                let mut leaves = [[0u8; DIGEST_LEN]; MAX_LEAVES];
                // Retrieve the leaves of the Merkle tree into the array.
                let leaves = self.leaves(tree, &mut leaves)?;
                let (index, mut path) = (usize::from(index), [[0u8; DIGEST_LEN]; MAX_PROOF]);
                // Generate the Merkle proof for the specified leaf index.
                let steps = proof(leaves, index, &mut path).ok_or(Error::UnknownCommitment)?;
                let leaf = leaves.get(index).ok_or(Error::UnknownCommitment)?;
                let path = path.get(..steps).unwrap_or(&[]).as_flattened();
                // Fill the output buffer with the leaf, the number of steps, and the proof path.
                fill(out, &[leaf, &[steps as u8; 2], path])
            }
            [GET_MERKLE_LEAF_INDEX, query @ ..] => {
                // Get the top of the Merkle tree and the leaf to find from the client command.
                let (top, wanted) = query
                    .split_first_chunk::<DIGEST_LEN>()
                    .ok_or(Error::UnknownCommitment)?;
                let mut leaves = [[0u8; DIGEST_LEN]; MAX_LEAVES];
                // Retrieve the leaves of the Merkle tree corresponding to the top hash.
                let leaves = self.leaves(self.find(top, None)?, &mut leaves)?;
                // Find the index of the wanted leaf in the retrieved leaves.
                let found = leaves.iter().position(|leaf| leaf == wanted);
                // Fill the output buffer with the result of the search.
                fill(out, &[&found.map_or([0, 0], |index| [1, index as u8])])
            }
            [other, ..] => Err(Error::UnexpectedClientCommand(*other)),
            [] => Err(Error::ResponseTruncated),
        }
    }

    /// Returns the signed transaction if available, otherwise returns an error.
    pub fn signed(&self, out: &mut [u8]) -> Result<usize> {
        // Get the signature from the stored signature buffer, up to the current signature length.
        let signature = self.signature.get(..self.signature_len).unwrap_or(&[]);
        match self.signing {
            // Check if there is a signing context and a non-empty signature.
            Some((draft, key)) if !signature.is_empty() => {
                Ok(signed(&draft, &key, signature, out)?)
            }
            _ => Err(Error::MissingSignature),
        }
    }

    /// Processes a yield command from the client, storing the provided signature.
    fn take(&mut self, yielded: &[u8]) -> Result<usize> {
        let refused = Error::UnexpectedClientCommand(YIELD);
        // Ensure that the yielded command has the expected format and contains a valid signature.
        let (Some((_, key)), [0, key_len, rest @ ..]) = (self.signing, yielded) else {
            return Err(refused);
        };
        // Split the rest of the yielded command into the key and the signature.
        let (yielded_key, signature) = rest.split_at_checked((*key_len).into()).ok_or(refused)?;
        // Get a mutable reference to the slot where the signature will be stored.
        let slot = self.signature.get_mut(..signature.len()).ok_or(refused)?;
        if yielded_key != key || signature.is_empty() {
            return Err(refused);
        }
        // Copy the signature into the allocated slot and update the signature length.
        slot.copy_from_slice(signature);
        // Update the signature length to reflect the newly stored signature.
        self.signature_len = signature.len();
        Ok(0)
    }

    /// Returns the number of elements in the specified Merkle tree.
    fn count(&self, tree: Tree) -> usize {
        // if there is no signing context, only the wallet keys tree has one element
        let Some((draft, _)) = self.signing else {
            // return one element
            return usize::from(tree == Tree::Wallet);
        };
        // if there is a change then that is one extra element
        let change = usize::from(draft.change > 0);
        match tree {
            // these only have one element
            Tree::Wallet | Tree::GlobalKeys | Tree::GlobalValues | Tree::Inputs => 1,
            // witness, derivation, txid and index is part of these trees so it has four elements
            Tree::InputKeys | Tree::InputValues => 4,
            // amount and script so it has two elements
            Tree::RecipientKeys | Tree::RecipientValues => 2,
            // amount and script for the change output, so 2 per change output
            Tree::ChangeKeys | Tree::ChangeValues => 2 * change,
            // outputs include the regular outputs plus any change outputs
            Tree::Outputs => 1 + change,
        }
    }

    /// Returns the element at the specified index in the specified Merkle tree.
    fn element(&self, tree: Tree, index: usize, out: &mut [u8]) -> Result<usize> {
        let missing = Error::UnknownCommitment;
        // check if the requested index is within bounds
        if index >= self.count(tree) {
            return Err(missing);
        }
        // handle the wallet tree separately
        if tree == Tree::Wallet {
            return fill(out, &[&self.key]);
        }

        // get the signing context and key, if we arent signing probably
        // we are only requesting an address and therefore we shouldnt be requesting
        // information that requires signing.
        let (draft, key) = self.signing.ok_or(missing)?;
        let sender = draft.sender();
        match (tree, index) {
            (Tree::Wallet, _) => unreachable!("Wallet tree should have been handled separately"), 
            (Tree::GlobalKeys, _) => fill(out, &[&[GLOBAL_TX_VERSION]]), // tx version key
            (Tree::GlobalValues, _) => fill(out, &[&TX_VERSION.to_le_bytes()]), // tx version serialized
            (Tree::InputKeys, 0) => fill(out, &[&[IN_WITNESS_UTXO]]), // the inputs witness utxo key
            (Tree::InputKeys, 1) => fill(out, &[&[IN_BIP32_DERIVATION], &key]), // the input position in Ledger key tree
            (Tree::InputKeys, 2) => fill(out, &[&[IN_PREVIOUS_TXID]]), // previous outpoint key
            (Tree::InputKeys, _) => fill(out, &[&[IN_OUTPUT_INDEX]]), // output index key
            (Tree::InputValues, 0) => {
                // input amount and sender
                let amount = draft.input_amount.to_le_bytes();
                fill(out, &[&amount, &[sender.len() as u8], sender])
            }
            (Tree::InputValues, 1) => fill(out, &[&self.fingerprint, PATH.as_flattened()]), // input BIP32 derivation path
            (Tree::InputValues, 2) => fill(out, &[&draft.txid]), // previous txid
            (Tree::InputValues, _) => fill(out, &[&draft.vout.to_le_bytes()]), // output index
            (Tree::RecipientKeys | Tree::ChangeKeys, 0) => fill(out, &[&[OUT_AMOUNT]]), // output amount key
            (Tree::RecipientKeys | Tree::ChangeKeys, _) => fill(out, &[&[OUT_SCRIPT]]), // output script key
            (Tree::RecipientValues, 0) => fill(out, &[&draft.amount.to_le_bytes()]), // output amount value
            (Tree::RecipientValues, _) => fill(out, &[draft.recipient()]), // output script value
            (Tree::ChangeValues, 0) => fill(out, &[&draft.change.to_le_bytes()]), // change amount value
            (Tree::ChangeValues, _) => fill(out, &[sender]), // change script value
            (Tree::Inputs, _) => fill(
                out,
                &[&self.commitment(Tree::InputKeys, Tree::InputValues)?], // compute merkle commitment for inputs
            ),
            (Tree::Outputs, 0) => fill(
                out,
                &[&self.commitment(Tree::RecipientKeys, Tree::RecipientValues)?], // compute merkle commitment for recipient outputs
            ),
            (Tree::Outputs, _) => fill(
                out,
                &[&self.commitment(Tree::ChangeKeys, Tree::ChangeValues)?], // compute merkle commitment for change outputs
            ),
        }
    }

    /// Compute the leaf hash for a given tree and index
    fn leaf(&self, tree: Tree, index: usize) -> Result<Hash> {
        let mut element = [0u8; ELEMENT_MAX];
        // Get the element for the given tree and index
        let len = self.element(tree, index, &mut element)?;
        // Compute the leaf hash using the element and its length
        Ok(digest(&[&[0x00], element.get(..len).unwrap_or(&[])]))
    }

    /// Compute the leaf hashes for all elements in a given tree
    fn leaves<'a>(&self, tree: Tree, out: &'a mut [Hash; MAX_LEAVES]) -> Result<&'a [Hash]> {
        // Get the number of elements in the tree
        let count = self.count(tree);
        // Compute the leaf hash for each element in the tree
        for (index, leaf) in out.iter_mut().enumerate().take(count) {
            *leaf = self.leaf(tree, index)?;
        }
        // Return the computed leaf hashes
        Ok(out.get(..count).unwrap_or(&[]))
    }

    /// Compute the Merkle root of a given tree
    fn root_of(&self, tree: Tree) -> Result<Hash> {
        // Compute the leaf hashes for the tree
        let mut leaves = [[0u8; DIGEST_LEN]; MAX_LEAVES];
        Ok(root(self.leaves(tree, &mut leaves)?))
    }

    /// Compute the Merkle commitment for a set of keys and values
    fn commitment(&self, keys: Tree, values: Tree) -> Result<[u8; COMMITMENT_LEN]> {
        // Get the count of keys and compute the Merkle roots for keys and values
        let count = [self.count(keys) as u8];
        // Compute the Merkle roots for the keys and values
        let (names, contents) = (self.root_of(keys)?, self.root_of(values)?);
        let mut commitment = [0u8; COMMITMENT_LEN];
        // Fill the commitment with the count, names, and contents
        // committing to the combination of count, names, and contents
        fill(&mut commitment, &[&count, &names, &contents])?;
        Ok(commitment)
    }

    /// Find a tree that matches the given top hash and optional size
    fn find(&self, top: &[u8], size: Option<u8>) -> Result<Tree> {
        // Iterate over all known trees to find a match
        for tree in TREES {
            // Get the count of elements in the current tree
            let count = self.count(tree);
            // Determine if the current tree fits the specified size, if any
            let fits = size.is_none_or(|size| usize::from(size) == count);
            // Check if the tree has elements, fits the size, and matches the top hash
            if count > 0 && fits && self.root_of(tree)? == top {
                return Ok(tree);
            }
        }
        // Return an error if no matching tree is found
        Err(Error::UnknownCommitment)
    }

    /// Locate a leaf hash within the known trees and return its tree and index
    fn locate(&self, hash: &[u8]) -> Result<(Tree, usize)> {
        // Iterate over all known trees to find the leaf hash
        for tree in TREES {
            for index in 0..self.count(tree) {
                // Check if the current leaf matches the given hash
                if self.leaf(tree, index)? == hash {
                    return Ok((tree, index));
                }
            }
        }
        // Return an error if the leaf hash is not found
        Err(Error::UnknownCommitment)
    }
}

pub fn public_key(xpub: &[u8]) -> Result<[u8; PUBLIC_KEY_LEN]> {
    let mut decoded = [0u8; 82];
    for character in xpub {
        let digit = BASE58.iter().position(|known| known == character);
        let mut carry = digit.ok_or(Error::MalformedKey)? as u32;
        for byte in decoded.iter_mut().rev() {
            carry += u32::from(*byte) * 58;
            *byte = carry as u8;
            carry >>= 8;
        }
        if carry != 0 {
            return Err(Error::MalformedKey);
        }
    }
    let Some((payload, checksum)) = decoded.split_last_chunk::<XPUB_CHECKSUM_LEN>() else {
        return Err(Error::MalformedKey);
    };
    let key: [u8; PUBLIC_KEY_LEN] = payload
        .get(KEY_AT..KEY_AT + PUBLIC_KEY_LEN)
        .and_then(|key| key.try_into().ok())
        .ok_or(Error::MalformedKey)?;
    let sum = digest(&[&digest(&[payload])]);
    match (sum.first_chunk() == Some(checksum), key.first()) {
        (true, Some(0x02 | 0x03)) => Ok(key), // compressed keys only, see https://github.com/bitcoin/bips/blob/b34d9f751791/bip-0143.mediawiki?plain=1#L131
        _ => Err(Error::MalformedKey),
    }
}

/// Compute the Merkle root of a set of leaves
fn root(leaves: &[Hash]) -> Hash {
    match leaves {
        [] => [0; DIGEST_LEN],
        [only] => *only,
        _ => {
            // Split the leaves into left and right halves for recursive computation
            let (left, right) = halves(leaves);
            // Recursively compute the Merkle root of the left and right halves
            digest(&[&[0x01], &root(left), &root(right)])
        }
    }
}

/// Compute the Merkle proof for a leaf at a given index
fn proof(leaves: &[Hash], index: usize, out: &mut [Hash]) -> Option<usize> {
    // Return early if there are fewer than two leaves, as no proof is needed
    if leaves.len() < 2 {
        return (index < leaves.len()).then_some(0);
    }
    // Split the leaves into left and right halves for recursive proof computation
    // if possible, otherwise we reached the leaf level
    let (left, right) = halves(leaves);
    // Determine which half contains the leaf at the given index and compute the proof recursively
    let (written, sibling) = match index.checked_sub(left.len()) {
        None => (proof(left, index, out)?, root(right)),
        Some(index) => (proof(right, index, out)?, root(left)),
    };
    // Append the sibling hash to the proof array
    *out.get_mut(written)? = sibling;
    Some(written + 1)
}

/// Split a slice of leaves into left and right halves, 
/// ensuring the split is a power of two, essentially
/// as slong as we can split by 2
fn halves(leaves: &[Hash]) -> (&[Hash], &[Hash]) {
    let split = leaves.len().next_power_of_two() / 2;
    leaves.split_at_checked(split).unwrap_or((leaves, &[]))
}

/// Compute the digest of the given parts using SHA-256
fn digest(parts: &[&[u8]]) -> Hash {
    let mut hasher = Sha256::new();
    parts.iter().for_each(|part| hasher.update(part));
    hasher.finalize().into()
}

/// Fill the output buffer with the concatenated parts, returning the number of bytes written
fn fill(out: &mut [u8], parts: &[&[u8]]) -> Result<usize> {
    let (needed, found) = (parts.iter().map(|part| part.len()).sum(), out.len());
    put(out, parts).ok_or(Error::OutputTooSmall { needed, found })
}

#[cfg(test)]
mod tests {
    use super::{Client, Error, digest, public_key};
    use crate::bitcoin::Draft;

    const FINGERPRINT: [u8; 4] = [0x73, 0xc5, 0xda, 0x0a];
    const XPUB: &[u8] = b"xpub6CatWdiZiodmUeTDp8LT5or8nmbKNcuyvz7WyksVFkKB4RHwCD3XyuvPEbvqAQY3rAPshWcMLoP2fMFMKHPJ4ZeZXYVUhLv1VMrjPC7PW6V";
    const LEAF: &str = "fac4ff3981317caa5a3a3d56d7401a7c97e9cdf03f8a77038b7a10a9dde46244";
    const POLICY: &str = "02000bc8974a0d8bdd29024b2ddb7a7fe8df1d9801b270f4e6c1e7e1011ae39e7c9b0001fac4ff3981317caa5a3a3d56d7401a7c97e9cdf03f8a77038b7a10a9dde46244";
    const ID: &str = "ad9de30bf97a12adf70b45ece3890a8f1861c317f478ab3a80acdb59902252d4";
    const RECORD: &str = "626331716372387465346b723630396763617775746d727a61306a34787638306a79387a333036667975000000000001d4c000000000000013881f1e1d1c1b1a191817161514131211100f0e0d0c0b0a090807060504030201000000000100000000000249f06263317177353038643671656a7874646734793572337a6172766172793063357877376b763866337434";
    const PUBLIC_KEY: &str = "0330d54fd0dd420a6e5f8d3624f5f3482cae350f79d5f0753bf5beef9c2d91af3c";
    const REQUEST: &str = "01fcf0a6c700dd13e274b6fba8deea8dd9b26e4eedde3495717cac8408c9c5177f0bd288cecce2dfc6c9b9245ab747a10870f84c16e986e61b259603b59cf1f3b90184ac33617bde9f28007064d6235059cdb533e68d6c5c504c87891d89bf4373670267a6e5f777f2afc1462db26809d91cdce91fd53bc2700738c48bbb811ed5117aad9de30bf97a12adf70b45ece3890a8f1861c317f478ab3a80acdb59902252d40000000000000000000000000000000000000000000000000000000000000000";
    const INPUT_COMMITMENT: &str = "047bb3f75f2bfc32dffe9d87dc5e63672f45daa9d7049c4d644f6624394ae26725e640d7347c4bfa4e741f8f06f95ac79ef6dbfcd1d2ec36e0510994bb08a3a186";
    const SIGNATURE: &str = "30440220aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa0220bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb01";
    const SIGNED: &str = "02000000000101000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f0100000000ffffffff02c0d4010000000000160014751e76e8199196d454941c45d1b3a323f1433bd66f72000000000000160014c0cebcd6c3d3ca8c75dc5ec62ebe55330ef910e2024730440220aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa0220bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb01210330d54fd0dd420a6e5f8d3624f5f3482cae350f79d5f0753bf5beef9c2d91af3c00000000";

    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).expect("hex"))
            .collect()
    }

    fn ask(client: &mut Client, parts: &[&[u8]]) -> Result<Vec<u8>, Error> {
        let mut out = [0u8; 255];
        let len = client.answer(&parts.concat(), &mut out)?;
        Ok(out[..len].to_vec())
    }

    fn signing() -> (Client, [u8; 33]) {
        let draft = Draft::decode(&bytes(RECORD)).expect("valid draft");
        let key: [u8; 33] = bytes(PUBLIC_KEY).try_into().expect("33 bytes");
        let client = Client::new(&FINGERPRINT, XPUB, Some((draft, key))).expect("valid");
        (client, key)
    }

    #[test]
    fn the_wallet_policy_is_the_documented_default_wallet() {
        let mut client = Client::new(&FINGERPRINT, XPUB, None).expect("valid");
        assert_eq!(client.policy.to_vec(), bytes(POLICY));
        let key = [&b"[73c5da0a/84'/0'/0']"[..], XPUB].concat();
        for (hash, preimage) in [
            (bytes(ID), bytes(POLICY)),
            (digest(&[b"wpkh(@0/**)"]).to_vec(), b"wpkh(@0/**)".to_vec()),
            (bytes(LEAF), [&[0x00][..], &key].concat()),
        ] {
            let len = preimage.len() as u8;
            let expected = [&[len, len][..], &preimage].concat();
            assert_eq!(ask(&mut client, &[&[0x40, 0x00], &hash]), Ok(expected));
        }
        let leaf = bytes(LEAF);
        let proved = [&leaf[..], &[0, 0]].concat();
        assert_eq!(ask(&mut client, &[&[0x41], &leaf, &[1, 0]]), Ok(proved));
        assert!(Client::new(&FINGERPRINT[..3], XPUB, None).is_err());
        assert!(Client::new(&FINGERPRINT, &XPUB[..110], None).is_err());
    }

    #[test]
    fn signing_serves_every_field_the_device_reads_and_yields_the_reference_transaction() {
        let (mut client, key) = signing();
        assert_eq!(
            client.sign_request().expect("request").to_vec(),
            bytes(REQUEST)
        );

        let commitment = bytes(INPUT_COMMITMENT);
        let inputs = &bytes(REQUEST)[66..98];
        let proved = [&digest(&[&[0x00], &commitment])[..], &[0, 0]].concat();
        assert_eq!(ask(&mut client, &[&[0x41], inputs, &[1, 0]]), Ok(proved));
        let (keys, values) = (&commitment[1..33], &commitment[33..65]);
        let (held, absent) = (digest(&[&[0x00], &[0x0E]]), digest(&[&[0x00], &[0x10]]));
        assert_eq!(ask(&mut client, &[&[0x42], keys, &held]), Ok(vec![1, 2]));
        assert_eq!(ask(&mut client, &[&[0x42], keys, &absent]), Ok(vec![0, 0]));
        let txid: Vec<u8> = (0u8..32).collect();
        let txid_leaf = digest(&[&[0x00], &txid]);
        let value = ask(&mut client, &[&[0x41], values, &[4, 2]]).expect("known");
        assert_eq!(value[..34].to_vec(), [&txid_leaf[..], &[2, 2]].concat());
        let preimage = [&[33, 33, 0x00][..], &txid].concat();
        assert_eq!(ask(&mut client, &[&[0x40, 0x00], &txid_leaf]), Ok(preimage));

        let mut out = [0u8; 244];
        assert_eq!(client.signed(&mut out), Err(Error::MissingSignature));
        let yielded = [&[0x10, 0, 33][..], &key, &bytes(SIGNATURE)].concat();
        assert_eq!(ask(&mut client, &[&yielded]), Ok(vec![]));
        let len = client.signed(&mut out).expect("signed");
        assert_eq!(out[..len].to_vec(), bytes(SIGNED));
    }

    #[test]
    fn anything_the_client_does_not_hold_is_refused() {
        let (mut client, key) = signing();
        let (mut other, signature) = (key, bytes(SIGNATURE));
        other[1] ^= 1;
        let yield_with = |index: u8, key: &[u8], signature: &[u8]| {
            [&[0x10, index, key.len() as u8][..], key, signature].concat()
        };
        let (unknown, unexpected) = (Error::UnknownCommitment, Error::UnexpectedClientCommand);
        for (command, error) in [
            ([&[0x40, 0x00][..], &[0x11; 32]].concat(), unknown),
            ([&[0x41][..], &[0x11; 32], &[1, 0]].concat(), unknown),
            ([&[0x40, 0x01][..], &bytes(ID)].concat(), unexpected(0x40)),
            (yield_with(1, &key, &signature), unexpected(0x10)),
            (yield_with(0, &other, &signature), unexpected(0x10)),
            (yield_with(0, &key, &[]), unexpected(0x10)),
            (yield_with(0, &key, &[0; 74]), unexpected(0x10)),
            (vec![0xA0], unexpected(0xA0)),
            (vec![], Error::ResponseTruncated),
        ] {
            assert_eq!(ask(&mut client, &[&command]), Err(error), "{command:02x?}");
        }
    }

    #[test]
    fn every_merkle_proof_leads_to_its_root() {
        let leaves: Vec<[u8; 32]> = (0..4u8).map(|seed| digest(&[&[seed]])).collect();
        for size in 1..=4 {
            let top = super::root(&leaves[..size]);
            for index in 0..size {
                let mut path = [[0u8; 32]; 2];
                let steps = super::proof(&leaves[..size], index, &mut path).expect("in range");
                let (mut hash, mut at, mut width, mut rights) =
                    (leaves[index], index, size, vec![]);
                while width > 1 {
                    let left = width.next_power_of_two() / 2;
                    rights.push(at >= left);
                    (at, width) = if at >= left {
                        (at - left, width - left)
                    } else {
                        (at, left)
                    };
                }
                for (sibling, right) in path[..steps].iter().zip(rights.iter().rev()) {
                    hash = if *right {
                        digest(&[&[0x01], sibling, &hash])
                    } else {
                        digest(&[&[0x01], &hash, sibling])
                    };
                }
                assert_eq!(hash, top, "leaf {index} of {size}");
            }
        }
    }

    #[test]
    fn the_key_of_a_depth_five_xpub_is_its_compressed_public_key() {
        let xpub = b"xpub6H1LXWLaKsWFhvm6RVpEL9P4KfRZSW7abD2ttkWP3SSQvnyA8FSVqNTEcYFgJS2UaFcxupHiYkro49S8yGasTvXEYBVPamhGW6cFJodrTHy";
        let expected = bytes("022a471424da5e657499d1ff51cb43c47481a03b1e77f951fe64cec9f5a48f7011");
        assert_eq!(public_key(xpub).map(|key| key.to_vec()), Ok(expected));
        assert_eq!(public_key(b"xpub0"), Err(Error::MalformedKey));
    }

    #[test]
    fn a_damaged_xpub_or_an_uncompressed_key_is_refused() {
        let mut damaged = b"xpub6H1LXWLaKsWFhvm6RVpEL9P4KfRZSW7abD2ttkWP3SSQvnyA8FSVqNTEcYFgJS2UaFcxupHiYkro49S8yGasTvXEYBVPamhGW6cFJodrTHy".to_vec();
        damaged[100] = b'2';
        let uncompressed = b"xpub6H1LXWLaKsWFhvm6RVpEL9P4KfRZSW7abD2ttkWP3SSQvnyA8FSVqNTEcc9AetvYH6yRBTT8Mv6nxndgoVpJmLCz7Tdc5554VQd2J5QoxHf";
        for bad in [&damaged[..], uncompressed] {
            assert_eq!(public_key(bad), Err(Error::MalformedKey));
        }
    }
}
