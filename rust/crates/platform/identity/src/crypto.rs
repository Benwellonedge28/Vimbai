//! The two primitives the service leaned on: SHA-256 for token derivation and
//! unpadded base64url for JWT payloads.
//!
//! SHA-256 lives in `vimbai_common::hash`, because the integrity checks hash with it
//! too and one compression function is enough for both. Base64url stays here: it is
//! only the JWT payload that needs it.

/// SHA-256, hexadecimal. Used to derive the Vimbai access token.
pub fn sha256_hex(input: &str) -> String {
    vimbai_common::hash::sha256_hex(input.as_bytes())
}

/// Unpadded base64url decode. The service padded with `4 - len % 4` equals
/// signs, which Python's decoder tolerated; equivalent here.
pub fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    fn sextet(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'-' | b'+' => Some(62),
            b'_' | b'/' => Some(63),
            b'=' => Some(0),
            _ => None,
        }
    }

    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buffer: u32 = 0;
    let mut bits = 0u8;
    let mut seen_padding = false;
    for byte in input.bytes() {
        if byte == b'=' {
            seen_padding = true;
            continue;
        }
        if seen_padding {
            return None; // data after padding is malformed
        }
        let value = sextet(byte)?;
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

/// base64url without padding, for the JWT segments the crate emits in tests.
pub fn base64url_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map(u32::from).unwrap_or(0);
        let b2 = chunk.get(2).copied().map(u32::from).unwrap_or(0);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(triple >> 18) as usize & 0x3f] as char);
        out.push(ALPHABET[(triple >> 12) as usize & 0x3f] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(triple >> 6) as usize & 0x3f] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[triple as usize & 0x3f] as char);
        }
    }
    out
}
