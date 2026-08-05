//! Base64url, unpadded — the `aft` parameter value's encoding ([`affinity-token`] §5).
//!
//! RFC 4648 §5. The alphabet (`A–Z a–z 0–9 - _`) falls entirely inside RFC 3261 §25.1
//! `unreserved`, so a token needs no percent-escaping, and dropping the padding removes `=`, which
//! would need escaping. Written here rather than taken from a crate for the same reason
//! `sipx-clstr-probe` writes its own: the encoder is twenty lines, and the **decoder** has to
//! refuse things a general-purpose one accepts — §5 makes padding and out-of-alphabet bytes hard
//! rejections, before §8 S1 is ever reached (`AT-16`).
//!
//! [`affinity-token`]: https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/affinity-token.md

use thiserror::Error;

use crate::token::MAX_TOKEN_LEN;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// The affinity-token URI parameter name (§5).
///
/// Registered lowercase and matched case-insensitively per RFC 3261 §19.1.4. Exactly one per
/// platform URI: a `Route` resolving here with zero or several fails verification, because "which
/// token did you mean" has no safe answer.
///
/// It lives beside the encoding rather than in the forwarding core because §5 owns both — the
/// parameter name, the alphabet and the budget below are one rule, and the crate that can *measure*
/// a token is the only one that can prove the budget holds.
pub const TOKEN_PARAM: &str = "aft";

/// The normative size budget for the token **parameter** — not the header line (§5, proxy-behavior
/// §7 F4).
///
/// The distinction is this spec's own correction to `PB-F-1`'s shorthand: at the module-facts
/// ceiling the whole `Record-Route` line exceeds 200 B for any realistic host, while the parameter
/// stays inside it (157 B, `AT-6`). Asserting on the line would fail a compliant token.
pub const TOKEN_PARAM_BUDGET: usize = 200;

/// How many characters unpadded base64url needs for `len` bytes (§5's arithmetic: `4·⌊n/3⌋` plus
/// 0/2/3 for `n mod 3` = 0/1/2).
///
/// `const` so [`WORST_CASE_PARAM_LEN`] can be computed at compile time, which is what turns the
/// budget from a number two crates repeat into one the build proves.
#[must_use]
pub const fn encoded_len(len: usize) -> usize {
    match len % 3 {
        0 => len / 3 * 4,
        1 => len / 3 * 4 + 2,
        _ => len / 3 * 4 + 3,
    }
}

/// The largest `;aft=<value>` this layout can produce: `MAX_TOKEN_LEN` bytes encoded, plus `;aft=`.
///
/// §5 tabulates it as **157 B** — encrypted mode at the 64-byte module-facts ceiling.
pub const WORST_CASE_PARAM_LEN: usize = ";=".len() + TOKEN_PARAM.len() + encoded_len(MAX_TOKEN_LEN);

// `TOKEN_PARAM_BUDGET` and `MAX_TOKEN_LEN` used to be two unrelated numbers in two crates, and
// nothing compared them: the budget was a literal in the forwarding core, the ceiling a literal
// here, and `AT-6` asserted `≤ 200` against a third literal of its own. Widening the layout — a
// bigger module-fact sub-budget, a longer tag — would have silently spent the headroom §5 measured
// and been found on the wire. This is the comparison, made by the compiler: the budget is satisfied
// by construction, or the workspace does not build.
const _: () = assert!(
    WORST_CASE_PARAM_LEN <= TOKEN_PARAM_BUDGET,
    "affinity-token §5: the worst-case `aft` parameter must fit proxy-behavior §7 F4's budget"
);

/// Why a parameter value is not a token at all.
///
/// A decode failure is a verification failure like any other — `403` on a mid-dialog request — but
/// it happens *before* [`verify`](crate::verify) sees bytes, so it is a separate type rather than a
/// [`Reason`](crate::Reason): there is no token to have a reason about yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum DecodeError {
    /// A byte outside the base64url alphabet — including `=`, which §5 forbids explicitly.
    #[error("byte {byte:#04x} at offset {offset} is not in the unpadded base64url alphabet")]
    NotInAlphabet {
        /// The offending byte.
        byte: u8,
        /// Where it was.
        offset: usize,
    },
    /// A length no unpadded base64url value can have: `len % 4 == 1` encodes a fractional byte.
    #[error("{len} characters cannot be unpadded base64url")]
    BadLength {
        /// How many characters there were.
        len: usize,
    },
}

/// The six-bit value a base64url character stands for, or `None` if it is not one.
///
/// A match rather than a search through [`ALPHABET`]: it is the same table read the other way, and
/// it keeps the decoder free of the width casts that a shift accumulator would need.
const fn digit_of(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

/// Encode bytes as unpadded base64url.
#[must_use]
pub fn encode_param_value(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = *chunk.first().unwrap_or(&0);
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let digits = [
            b0 >> 2,
            ((b0 & 0x03) << 4) | (b1 >> 4),
            ((b1 & 0x0f) << 2) | (b2 >> 6),
            b2 & 0x3f,
        ];
        // 4 characters per 3 input bytes, minus one per missing byte: 3 → 4, 2 → 3, 1 → 2.
        for digit in digits.iter().take(chunk.len() + 1) {
            out.push(char::from(
                *ALPHABET.get(usize::from(*digit)).unwrap_or(&b'A'),
            ));
        }
    }
    out
}

/// Decode an unpadded base64url parameter value.
///
/// Strict per §5: padding characters and any byte outside the alphabet are rejected. Trailing bits
/// of a non-multiple-of-3 value are ignored rather than required to be zero — a value whose length
/// is right and whose alphabet is right decodes to a byte string, and whether that byte string is a
/// token is [`verify`](crate::verify)'s question, not this one's.
///
/// # Errors
///
/// [`DecodeError`].
pub fn decode_param_value(value: &str) -> Result<Vec<u8>, DecodeError> {
    let raw = value.as_bytes();
    // Alphabet before length, deliberately. A padded value is *also* a value of a length unpadded
    // base64url cannot have, and reporting that would blame the length for what is really the `=`
    // §5 forbids — a telemetry line that sends the next operator looking in the wrong place.
    let mut out = Vec::with_capacity(raw.len() / 4 * 3);
    for (group, chunk) in raw.chunks(4).enumerate() {
        let mut digits = [0u8; 4];
        for (position, byte) in chunk.iter().enumerate() {
            let Some(digit) = digit_of(*byte) else {
                return Err(DecodeError::NotInAlphabet {
                    byte: *byte,
                    offset: group * 4 + position,
                });
            };
            if let Some(slot) = digits.get_mut(position) {
                *slot = digit;
            }
        }
        // Four six-bit digits pack into three bytes; a short final group carries one fewer byte per
        // missing digit, and a group of one digit is the length §5 cannot produce.
        let [d0, d1, d2, d3] = digits;
        let bytes = [(d0 << 2) | (d1 >> 4), (d1 << 4) | (d2 >> 2), (d2 << 6) | d3];
        match chunk.len() {
            1 => return Err(DecodeError::BadLength { len: raw.len() }),
            n => out.extend_from_slice(bytes.get(..n - 1).unwrap_or_default()),
        }
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, reason = "test module")]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_tail_length() {
        for len in 0..=16_u8 {
            let bytes: Vec<u8> = (0..len).collect();
            let encoded = encode_param_value(&bytes);
            assert!(!encoded.contains('='), "padding leaked into {encoded}");
            assert_eq!(decode_param_value(&encoded).unwrap(), bytes);
        }
    }

    #[test]
    fn matches_the_rfc_4648_test_vectors() {
        assert_eq!(encode_param_value(b""), "");
        assert_eq!(encode_param_value(b"f"), "Zg");
        assert_eq!(encode_param_value(b"fo"), "Zm8");
        assert_eq!(encode_param_value(b"foo"), "Zm9v");
        assert_eq!(encode_param_value(b"foob"), "Zm9vYg");
        assert_eq!(encode_param_value(b"fooba"), "Zm9vYmE");
        assert_eq!(encode_param_value(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn the_encoded_length_matches_what_the_encoder_produces() {
        // `encoded_len` is `const`, so nothing can call the encoder to check it; this is that check,
        // across every tail residue and at the ceiling §5 tabulates.
        for len in 0..=MAX_TOKEN_LEN {
            let bytes = vec![0x5a_u8; len];
            assert_eq!(
                encoded_len(len),
                encode_param_value(&bytes).len(),
                "encoded_len disagrees with the encoder at {len} bytes"
            );
        }
    }

    #[test]
    fn the_worst_case_parameter_is_the_157_bytes_the_spec_tabulates() {
        // affinity-token §5's budget table, bottom row of the encrypted mode: 114 raw bytes → 152
        // characters → a 157-byte parameter, 43 B inside F4's 200.
        assert_eq!(MAX_TOKEN_LEN, 114);
        assert_eq!(encoded_len(MAX_TOKEN_LEN), 152);
        assert_eq!(WORST_CASE_PARAM_LEN, 157);
        assert_eq!(TOKEN_PARAM_BUDGET, 200);
    }

    #[test]
    fn rejects_padding_and_foreign_bytes() {
        assert_eq!(
            decode_param_value("Zm9v="),
            Err(DecodeError::NotInAlphabet {
                byte: b'=',
                offset: 4
            })
        );
        assert_eq!(
            decode_param_value("Zm+v"),
            Err(DecodeError::NotInAlphabet {
                byte: b'+',
                offset: 2
            })
        );
        assert_eq!(
            decode_param_value("Z"),
            Err(DecodeError::BadLength { len: 1 })
        );
    }
}
