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
#[derive(Debug, Default, Clone)]
pub struct InMemoryCredentials {
    entries: Vec<(String, String, String)>,
}

impl InMemoryCredentials {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a password for a user of a tenant.
    #[must_use]
    pub fn with(
        mut self,
        tenant: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        self.entries
            .push((tenant.into(), username.into(), password.into()));
        self
    }
}

impl CredentialStore for InMemoryCredentials {
    fn password(&self, tenant: &str, username: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|(t, u, _)| t == tenant && u == username)
            .map(|(_, _, password)| password.clone())
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
        // a placeholder rather than returning early, so the answer is identical in content *and* in
        // the work done to produce it: an early return would make "no such user" measurably faster
        // than "wrong password", which is a user-enumeration oracle built out of a stopwatch.
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

    #[test]
    fn an_open_tenant_holds_no_realm_to_challenge_in() {
        let auth = TenantAuth::open("acme");
        assert_eq!(auth.realm(), "");
        assert_eq!(auth.tenant(), "acme");
    }
}
