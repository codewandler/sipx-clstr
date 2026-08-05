//! Sans-IO affinity tokens: mint, encode, parse, verify, with key rotation.
//!
//! A dialog's routing state rides inside the message. The edge that record-routes a dialog-forming
//! request mints one token per side, encodes it into the `aft` URI parameter of its `Record-Route`
//! entry, and every mid-dialog request thereafter presents it back in a `Route`. Any edge in the
//! cluster verifies it with the key set it already holds and learns the tenant, the direction, the
//! home shard, the media node and the policy version — **with no lookup anywhere**. That is
//! AGENTS.md rule 5 made mechanical, and M2's defining criterion.
//!
//! The contract is
//! [`docs/specs/affinity-token.md`](https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/affinity-token.md)
//! §2–§10, byte for byte; §10's vector table is executed verbatim by
//! `tests/vectors_affinity_token.rs`. The spec's §11–§14 flow references share this crate's key
//! set, algorithms and AEAD call site and land here with `AF-7`.
//!
//! # What this crate deliberately cannot do
//!
//! - **Read a clock.** `now` is an argument of [`verify`]; `expiry` is an argument of [`mint`].
//! - **Reach entropy.** [`mint`] takes the nonce as data. [`NonceSource`] is the seam a driver or
//!   the deterministic harness implements; the `chacha20poly1305` dependency is built with
//!   `default-features = false` precisely so its `getrandom` feature cannot put an operating-system
//!   source inside a sans-IO crate.
//! - **Remember anything.** Verification is stateless (§9). There is no nonce ledger and no
//!   seen-token store, and an implementation MUST NOT add one: re-presenting the same token on
//!   every mid-dialog request *is* the mechanism, and a replay store would put back exactly the
//!   hot-path global state the token exists to remove.
//!
//! # Considered for upstream (AGENTS.md rule 6): **no**
//!
//! Mint and verify over this cluster's routing state is orchestration, not protocol-generic. The
//! token's fields name platform concepts — tenants, shards, edges, media nodes, module facts — and
//! its carriage uses the kernel's existing URI-parameter surface unchanged, so there is nothing
//! here for `sipx` to own. Same answer, same reasoning, as affinity-token §1's own upstream note;
//! nothing joins [`docs/upstream.md`](https://github.com/codewandler/sipx-clstr/blob/main/docs/upstream.md).
//!
//! # Example
//!
//! ```
//! use bytes::Bytes;
//! use sipx_clstr_affinity::{
//!     Algorithm, Claims, Direction, Expect, KeyEntry, KeySet, MintKey, Verdict, mint, verify,
//! };
//!
//! let key = MintKey::new(1, Algorithm::ChaCha20Poly1305, [0x11; 32]);
//! let keys = KeySet::new(vec![KeyEntry::new(key.clone(), 0, u32::MAX, true)]).unwrap();
//!
//! let claims = Claims {
//!     tenant: 7,
//!     home_shard: 3,
//!     edge: 5,
//!     direction: Direction::Originating,
//!     media_node: 9,
//!     policy_version: 41,
//!     expiry: 1_785_326_400,
//!     module_facts: Bytes::new(),
//! };
//!
//! let token = mint(&claims, &key, [0xa0; 12]).unwrap();
//! let parameter = token.to_param_value(); // the `aft` value, base64url unpadded
//!
//! // Any edge, any number of times, no shared state.
//! let verdict = verify(token.as_bytes(), &keys, 1_785_240_060, &Expect::new());
//! assert_eq!(verdict, Verdict::Valid(claims));
//! # let _ = parameter;
//! ```

#![doc(html_no_source)]

mod codec;
mod keys;
mod token;

pub use codec::{
    DecodeError, TOKEN_PARAM, TOKEN_PARAM_BUDGET, WORST_CASE_PARAM_LEN, decode_param_value,
    encode_param_value, encoded_len,
};
pub use keys::{Algorithm, KeyEntry, KeySet, KeySetError, MintKey, retirement_deadline};
pub use token::{
    Claims, DEFAULT_LIFETIME, DEFAULT_SKEW, Direction, Expect, LIFETIME_FLOOR, MAX_FACTS,
    MAX_TOKEN_LEN, MIN_TOKEN_LEN, MintError, NonceSource, Reason, Token, VERSION, Verdict, mint,
    mint_with, verify,
};
