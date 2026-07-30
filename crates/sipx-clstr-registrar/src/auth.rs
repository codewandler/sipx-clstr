//! Whether this edge believes who a REGISTER says it is — [registrar-auth](
//! https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/registrar-auth.md) §3.
//!
//! The digest **primitives** are the kernel's ([`sipx_ua::challenge`], upstream `S-16`): nonce
//! minting, the hash formulas, verification, the replay window. This module is the **policy** —
//! which tenants require authentication and in which realm, where credentials come from, what a
//! refusal says, and the identity a success is recorded under. The split is §2's, and the reason
//! is that two implementations of one algorithm eventually disagree about who is authenticated,
//! and the one that disagrees quietly is a security bug.
//!
//! Sans-IO like the rest of this crate: `now` is an argument, and the nonce secret is supplied
//! rather than drawn, so a harness scenario replays byte for byte from its seed.

use std::collections::HashMap;

use bytes::Bytes;
use sipx_sip::{HeaderName, Request};
use sipx_ua::challenge::{Authenticator, Presented, Verdict};

pub use sipx_ua::Algorithm;
pub use sipx_ua::challenge::Reason;

/// Where the passwords for a tenant come from.
///
/// A trait because a credential store is a deployment's business, not this crate's — the same
/// reason the kernel's `verify` takes a password as an argument instead of looking one up.
pub trait CredentialStore {
    /// The password this tenant holds for `username`, if it holds one.
    ///
    /// Returning `None` must be indistinguishable *to the far end* from returning a wrong
    /// password; [`TenantAuth::decide`] is what guarantees that, not the implementation here.
    fn password(&self, tenant: &str, username: &str) -> Option<String>;
}

/// A credential store held in memory, for the harness and for single-tenant deployments.
///
/// Keyed rather than scanned (`RG-15`). It was a `Vec` walked with `find`, which is `O(users)` per
/// REGISTER **under the node-wide authenticator lock** — harmless while every deployment's
/// credentials were empty, and a throughput ceiling the moment one is not. Two maps rather than one
/// keyed on a pair, because a `HashMap<(String, String), _>` cannot be probed with `(&str, &str)`
/// without allocating a key per lookup, on the hot path, to answer a question about cost.
/// `Debug` is hand-written and prints **no passwords** — see the impl below.
#[derive(Default, Clone)]
pub struct InMemoryCredentials {
    tenants: HashMap<String, HashMap<String, String>>,
}

/// Counts, never contents.
///
/// A derived `Debug` here would put every plaintext password one `{:?}` away from a log line, and
/// §9 L2's whole point is that the guarantee should not depend on nobody ever writing that `{:?}`.
/// Same shape as `sipx-clstr-proxy`'s `CookieKey`, which redacts for the same reason. The counts are
/// kept because "the store has 0 users" is the diagnostic an operator actually needs, and it is not
/// a secret.
impl std::fmt::Debug for InMemoryCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let users: usize = self.tenants.values().map(HashMap::len).sum();
        f.debug_struct("InMemoryCredentials")
            .field("tenants", &self.tenants.len())
            .field("users", &users)
            .field("passwords", &"<redacted>")
            .finish()
    }
}

impl InMemoryCredentials {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a password for a user of a tenant.
    ///
    /// **The first password recorded for a `(tenant, username)` wins**, which is what the `Vec` and
    /// its `find` did. Preserved rather than tidied into last-wins: the two differ only for a
    /// caller that declares one user twice, and silently changing which credential such a caller
    /// authenticates against is not a change to make while nobody is looking.
    #[must_use]
    pub fn with(
        mut self,
        tenant: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        self.tenants
            .entry(tenant.into())
            .or_default()
            .entry(username.into())
            .or_insert_with(|| password.into());
        self
    }
}

impl CredentialStore for InMemoryCredentials {
    fn password(&self, tenant: &str, username: &str) -> Option<String> {
        self.tenants.get(tenant).and_then(|users| {
            // One credential slot, whatever the tenant holds: a hash probe examines at most one
            // entry, so this is where the meter's count stops depending on how full the store is.
            #[cfg(test)]
            lookup_meter::record();
            users.get(username).cloned()
        })
    }
}

/// A test-only meter over how many stored credentials one lookup has to touch (`RG-15`).
///
/// The same shape as [`crate::process`]'s parse meter, and for the same reason: the cost this
/// story is about is not visible in wall-clock time without flakiness, so it is counted instead.
/// A lookup whose count grows with the store is `O(users)` **under the node-wide authenticator
/// lock**, which is a throughput ceiling the moment a deployment has users.
///
/// **What this meter does and does not prove, stated plainly.** It caught the linear scan — the
/// call site was inside the `find` closure and reported 4096 against a store of 4096. It does
/// **not** prove the current implementation is `O(1)`: `record` is now called once per `password`,
/// before a single hash probe, so the tests below compare one with one and would do so for any
/// implementation that leaves the call where it is. The `O(1)` property rests on the [`HashMap`]
/// in the type, which is checkable by reading it; what the meter is worth now is that
/// reintroducing a per-entry scan at this call site turns red rather than silent. A meter that
/// counted a `HashMap`'s internal probes is not something the standard library exposes, and
/// inventing one would measure the meter rather than the store.
///
/// **A thread-local rather than an atomic.** `password` runs synchronously on its caller's thread,
/// so a thread-local lets each test see only its own lookups; a global atomic does not, because the
/// suite runs tests in parallel and a sibling's lookups leak into the delta.
#[cfg(test)]
pub(crate) mod lookup_meter {
    use std::cell::Cell;

    thread_local! {
        static TOUCHED: Cell<usize> = const { Cell::new(0) };
    }

    /// Forget every touch counted so far on this thread.
    pub(crate) fn reset() {
        TOUCHED.with(|count| count.set(0));
    }

    /// Count one stored credential examined.
    pub(crate) fn record() {
        TOUCHED.with(|count| count.set(count.get() + 1));
    }

    /// How many stored credentials have been examined on this thread since the last [`reset`].
    pub(crate) fn count() -> usize {
        TOUCHED.with(Cell::get)
    }
}

/// What the edge decided about a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// The request may become a `RegisterCommand`.
    ///
    /// `principal` is `None` when the tenant does not require authentication (§3 A1) — a recorded
    /// fact, so the audit trail can say *unauthenticated* rather than merely fail to say anything.
    Proceed {
        /// `<tenant>:<username>`, byte-exact as sent (§5).
        principal: Option<Bytes>,
    },
    /// Answer with a challenge.
    Challenge(ChallengeResponse),
    /// Answer `403`. The credentials named a protection space that is not this one (§3 A3);
    /// challenging again would loop.
    Forbidden,
}

impl Decision {
    /// This decision as the audit record §9 L1 requires, taken **before** the decision is consumed.
    ///
    /// A [`Decision`] is spent on the way to a `RegisterCommand`, and the record has to outlive
    /// that. `RG-15` shipped its first pass without this and left exactly the hole §9 L3 names: a
    /// REGISTER whose digest was correct and whose `Contact` then failed to parse produced no
    /// record at all, so a *proven* principal was indistinguishable from silence.
    #[must_use]
    pub fn outcome(&self) -> AuthOutcome {
        match self {
            // A5, and A1 — the difference is the whole of §9 L3.
            Decision::Proceed {
                principal: Some(principal),
            } => AuthOutcome::Authenticated(principal.clone()),
            Decision::Proceed { principal: None } => AuthOutcome::Unauthenticated,
            // A2 is not a refusal: nothing was wrong, because nothing had been offered. The split
            // lives here rather than in a driver so that every caller draws the line in one place.
            Decision::Challenge(challenge) if challenge.because.is_none() && !challenge.stale => {
                AuthOutcome::Challenged {
                    status: challenge.status,
                }
            }
            // A6 and A7.
            Decision::Challenge(challenge) => AuthOutcome::Refused {
                status: challenge.status,
                stale: challenge.stale,
                because: challenge.because,
            },
            // A3.
            Decision::Forbidden => AuthOutcome::Forbidden,
        }
    }
}

/// One entry of the authentication audit trail — [registrar-auth](
/// https://github.com/codewandler/sipx-clstr/blob/main/docs/specs/registrar-auth.md) §9.
///
/// Every outcome of §3 has a variant, so "exactly one record per outcome" (L1) is a property of the
/// type rather than of a driver remembering to write one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthOutcome {
    /// §3 A5 — the digest proved this principal (§5).
    Authenticated(Bytes),
    /// §3 A1 — the tenant does not require authentication, so nobody was authenticated and the
    /// trail says so out loud (L3).
    Unauthenticated,
    /// §3 A2 — challenged, with nothing yet offered to be wrong.
    Challenged {
        /// `401` for a registrar, `407` for a proxy.
        status: u16,
    },
    /// §3 A6 and A7 — refused, and challenged again with a fresh nonce.
    Refused {
        /// `401` for a registrar, `407` for a proxy.
        status: u16,
        /// Whether the fresh challenge carries `stale=true` (§3 A7).
        stale: bool,
        /// Why, from the kernel. `None` with `stale` is A7.
        because: Option<Reason>,
    },
    /// §3 A3 — the credentials named another protection space. `403`.
    Forbidden,
}

impl AuthOutcome {
    /// This outcome in the words the audit trail is allowed to use (§9 L2).
    ///
    /// **The return type is the guarantee.** `&'static str` cannot carry a nonce, a `cnonce`, a
    /// response digest, a presented username or a password, because none of those exist at compile
    /// time — so a driver that logs this cannot leak them however carelessly it writes the line.
    /// That is `StoreChoice::describe()`'s discipline, which returns a backend's name and never its
    /// resolved DSN, applied to the other end of the same problem: a log line is the artefact most
    /// likely to be copied into an issue, and here every input is attacker-controlled.
    ///
    /// It is defined for a *success* as well, so the guarantee covers the whole trail and not only
    /// its refusals.
    ///
    /// The match is exhaustive over the kernel's [`Reason`] on purpose. A `_` arm would make a
    /// variant added upstream silently render as "refused", and a reason that stops being reported
    /// is how an audit trail starts lying; this way the pin bump does not compile.
    #[must_use]
    pub fn describe(&self) -> &'static str {
        match self {
            AuthOutcome::Authenticated(_) => "the digest matched",
            AuthOutcome::Unauthenticated => "the tenant does not require authentication",
            AuthOutcome::Challenged { .. } => "no credentials were offered",
            // A7 before A6: an expiry arrives as `stale` with no reason attached, and reporting it
            // as a bad password is the confusion §3 A7 exists to prevent — users answer that by
            // changing passwords that were fine.
            AuthOutcome::Refused { stale: true, .. } => "the nonce had expired",
            AuthOutcome::Refused { because, .. } => match because {
                None => "refused without a stated reason",
                Some(Reason::Mismatch) => "the credentials did not match",
                Some(Reason::ForeignNonce) => "a nonce this edge did not mint",
                Some(Reason::Replay) => "a nonce-count that had already been used",
                Some(Reason::QopMismatch) => "credentials answered without qop=auth",
                Some(Reason::Algorithm) => "an algorithm this edge did not offer",
            },
            AuthOutcome::Forbidden => "the credentials named another protection space",
        }
    }

    /// Whether this outcome is one an operator should be alerted by, rather than merely recorded.
    ///
    /// A2 is **not**: it is the first half of a round trip the client is expected to complete, and
    /// every phone's ordinary first REGISTER takes it. Recording it as trouble buries the real
    /// thing under it.
    #[must_use]
    pub fn is_refusal(&self) -> bool {
        matches!(self, AuthOutcome::Refused { .. } | AuthOutcome::Forbidden)
    }
}

/// The challenge to put in a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeResponse {
    /// `401` for a registrar, `407` for a proxy.
    pub status: u16,
    /// `WWW-Authenticate` or `Proxy-Authenticate` — the twin of `status`, never mixed.
    pub header: HeaderName,
    /// The header value.
    pub value: String,
    /// Why, for logs and tests. `None` means "nothing was offered yet" (§3 A2).
    pub because: Option<Reason>,
    /// Whether the value carries `stale=true` (§3 A7).
    pub stale: bool,
}

/// One tenant's authentication policy, and the kernel authenticator that enforces it.
///
/// Holds the replay window, so it is `&mut` at the point of decision and lives as long as the edge
/// does. One per tenant: a shared secret across protection spaces would make a nonce issued for
/// one accepted in another.
#[derive(Debug)]
pub struct TenantAuth {
    tenant: String,
    authenticator: Authenticator,
    required: bool,
    proxy: bool,
}

impl TenantAuth {
    /// A tenant that requires digest authentication in `realm`, keyed by `secret`.
    ///
    /// `secret` must be stable across restarts for in-flight nonces to survive one, and must not be
    /// shared with another realm. Supplied rather than drawn so a scenario is reproducible; a
    /// deployment that has nowhere to keep one can generate it per start and pay a `stale=true`
    /// round trip after each restart.
    #[must_use]
    pub fn required(tenant: impl Into<String>, realm: impl Into<String>, secret: [u8; 32]) -> Self {
        Self {
            tenant: tenant.into(),
            authenticator: Authenticator::new(realm, secret),
            required: true,
            proxy: false,
        }
    }

    /// A tenant that does not authenticate. Every request proceeds with no principal (§3 A1).
    #[must_use]
    pub fn open(tenant: impl Into<String>) -> Self {
        Self {
            tenant: tenant.into(),
            // Never consulted, and keyed with zeros to make that visible: a secret that is only
            // decorative should look like one rather than like a weak real one.
            authenticator: Authenticator::new(String::new(), [0_u8; 32]),
            required: false,
            proxy: false,
        }
    }

    /// Offer this algorithm, and refuse credentials computed under any other (§4).
    ///
    /// **One** algorithm, not a menu. RFC 8760 §3: offering MD5 beside a modern algorithm invites a
    /// downgrade, because a challenge is not integrity-protected and an on-path attacker chooses
    /// which of several offers the client answers. A tenant that must interoperate with MD5-only
    /// endpoints sets MD5 *here*, so the weaker option is that tenant's and not everyone's.
    #[must_use]
    pub fn with_algorithm(mut self, algorithm: Algorithm) -> Self {
        self.authenticator = self.authenticator.with_algorithm(algorithm);
        self
    }

    /// How long a nonce stays usable.
    #[must_use]
    pub fn with_lifetime(mut self, lifetime: std::time::Duration) -> Self {
        self.authenticator = self.authenticator.with_lifetime(lifetime);
        self
    }

    /// Challenge as a proxy — `407`, `Proxy-Authenticate`, `Proxy-Authorization` — rather than as a
    /// registrar.
    ///
    /// The pair moves together and is never mixed: a server that challenges with `401` and reads
    /// `Proxy-Authorization` authenticates nobody while looking like it works.
    #[must_use]
    pub fn as_proxy(mut self) -> Self {
        self.proxy = true;
        self
    }

    /// The tenant this authenticates for.
    #[must_use]
    pub fn tenant(&self) -> &str {
        &self.tenant
    }

    /// The realm it challenges in.
    #[must_use]
    pub fn realm(&self) -> &str {
        self.authenticator.realm()
    }

    /// Decide §3, in its order.
    ///
    /// `now` is seconds since the epoch, supplied rather than read: this crate holds no clock.
    pub fn decide(
        &mut self,
        request: &Request,
        credentials: &impl CredentialStore,
        now: u64,
    ) -> Decision {
        // A1.
        if !self.required {
            return Decision::Proceed { principal: None };
        }

        // A2.
        let Some(presented) = Presented::from_request(request, self.proxy) else {
            return Decision::Challenge(self.challenge(false, None, now));
        };

        // A3. A realm is a protection space; answering a different one is not a wrong password, and
        // challenging again would loop between two ends that disagree about where they are.
        if presented.realm != self.realm() {
            return Decision::Forbidden;
        }

        // The digest covers the method (RFC 7616 §3.4.3), which is what stops an `Authorization`
        // captured from a REGISTER from authorizing an INVITE.
        let method = request.method.to_string();

        // A4. **A missing username takes the same path as a wrong one.** Verification runs against
        // a placeholder rather than returning early, so the answer is identical in content and the
        // SHA-256 work behind it is done either way: an early return here would make "no such user"
        // measurably faster than "wrong password", which is a user-enumeration oracle built out of
        // a stopwatch.
        //
        // Not a constant-time claim, and it was written as one until `RG-15`. The lookup itself is
        // a hash probe whose cost differs between a hit and a miss; what the placeholder buys is
        // that the *digest* runs regardless, which is the part that dominates. A store with a
        // genuinely data-dependent lookup would need more than this.
        //
        // Safe against the replay window too — the kernel records a nonce-count only on success, so
        // a guess at a username cannot spend counts a real client is about to use.
        let password = credentials
            .password(&self.tenant, &presented.username)
            .unwrap_or_else(|| ABSENT_USER_PLACEHOLDER.to_owned());

        match self
            .authenticator
            .verify_at(&presented, &method, &password, now)
        {
            // A5.
            Verdict::Authenticated => Decision::Proceed {
                principal: Some(self.principal(&presented.username)),
            },
            // A7.
            Verdict::Stale => Decision::Challenge(self.challenge(true, None, now)),
            // A6.
            Verdict::Rejected(reason) => {
                Decision::Challenge(self.challenge(false, Some(reason), now))
            }
        }
    }

    /// `<tenant>:<username>` (§5).
    ///
    /// The tenant is in it because a username is unique only within one, and a principal that could
    /// name two tenants' users is an authorization bug waiting for a cross-tenant lookup.
    fn principal(&self, username: &str) -> Bytes {
        Bytes::from(format!("{}:{username}", self.tenant))
    }

    fn challenge(&self, stale: bool, because: Option<Reason>, now: u64) -> ChallengeResponse {
        ChallengeResponse {
            status: if self.proxy { 407 } else { 401 },
            header: Authenticator::challenge_header(self.proxy),
            // A **fresh** nonce every time, including on a refusal. Re-offering the nonce the far
            // end just failed against would let a client retry a bad guess against the same
            // material indefinitely.
            value: self.authenticator.challenge_at(stale, now),
            because,
            stale,
        }
    }
}

/// Stands in for the password of a username the store does not hold.
///
/// Its value is irrelevant — no digest will match it — but its *existence* is what makes §3 A4's
/// "same path" literal rather than aspirational.
const ABSENT_USER_PLACEHOLDER: &str = "\0no such user\0";

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_principal_names_the_tenant_as_well_as_the_user() {
        let auth = TenantAuth::required("acme", "acme.example", [7_u8; 32]);
        assert_eq!(auth.principal("alice"), Bytes::from_static(b"acme:alice"));
        // Byte-exact, no case folding: two spellings of one username are two principals, and
        // deciding they are the same is an authorization question this layer does not answer.
        assert_eq!(auth.principal("Alice"), Bytes::from_static(b"acme:Alice"));
    }

    /// How many credentials the big store holds. The kernel's replay window is 4096 entries, and
    /// this is deliberately the same number: both sit under the same lock on the same request, and
    /// the point of the comparison is that neither may be walked to serve one REGISTER.
    const MANY: usize = 4096;

    /// A store of `count` users for one tenant, the last of which is `last`.
    fn a_store_of(count: usize, last: &str) -> InMemoryCredentials {
        let mut store = InMemoryCredentials::new();
        for index in 0..count.saturating_sub(1) {
            store = store.with("acme", format!("filler-{index}"), "irrelevant");
        }
        store.with("acme", last, "irrelevant")
    }

    /// How many stored credentials looking `username` up in `store` had to touch.
    fn cost_of_looking_up(store: &InMemoryCredentials, username: &str) -> usize {
        lookup_meter::reset();
        let _ = store.password("acme", username);
        lookup_meter::count()
    }

    /// **A failing-first test for `RG-15`.** The credential lookup's cost must not depend on how
    /// many credentials the tenant holds.
    ///
    /// It runs under the node-wide authenticator lock, once per authenticated REGISTER, which is
    /// what makes an `O(users)` scan a ceiling on the whole node rather than on one request. Both
    /// lookups ask for the **last** user in the store, so a linear scan pays its worst case and a
    /// lookup pays the same thing twice.
    ///
    /// It was red at the merge base — 1 against 4096 — and it is a **regression guard** now rather
    /// than a proof: see [`lookup_meter`] for exactly what it does and does not establish.
    #[test]
    fn the_credential_lookup_does_not_scan_the_tenant() {
        let small = a_store_of(1, "zoe");
        let big = a_store_of(MANY, "zoe");

        let in_a_small_store = cost_of_looking_up(&small, "zoe");
        let in_a_big_store = cost_of_looking_up(&big, "zoe");

        assert_eq!(
            in_a_small_store, in_a_big_store,
            "looking one user up touched {in_a_small_store} credentials in a store of 1 and \
             {in_a_big_store} in a store of {MANY}; the cost must not depend on how full it is"
        );
    }

    /// A username the store does not hold must cost the same as one it does — §3 A4's "same path".
    /// A scan that returns early on a hit makes an absent user the *expensive* case, which is the
    /// enumeration oracle pointing the other way.
    ///
    /// A guard and never a failing-first test: it passed at the merge base too, where both sides
    /// were 4096. See [`lookup_meter`] for its limits.
    #[test]
    fn an_absent_user_costs_what_a_present_one_costs() {
        let store = a_store_of(MANY, "zoe");

        let present = cost_of_looking_up(&store, "zoe");
        let absent = cost_of_looking_up(&store, "no-such-user");

        assert_eq!(
            present, absent,
            "a present user cost {present} and an absent one {absent}; §3 A4 wants one path"
        );
    }

    #[test]
    fn an_open_tenant_holds_no_realm_to_challenge_in() {
        let auth = TenantAuth::open("acme");
        assert_eq!(auth.realm(), "");
        assert_eq!(auth.tenant(), "acme");
    }
}
