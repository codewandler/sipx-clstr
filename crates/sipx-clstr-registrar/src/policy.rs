//! Pure REGISTER admission policy: served authorities and principal-to-AoR authorization.
//!
//! Digest proves an identity; it does not decide what that identity may register. The split is
//! deliberate: aliases, shared lines and administrative identities make username-to-AoR matching
//! an invalid authorization policy.

use bytes::Bytes;
use sipx_sip::{Host, Uri};

use crate::CanonicalAor;

/// The typed authority of a SIP/SIPS URI.
///
/// A user part is deliberately absent: it is not part of the authority. The optional port is kept
/// even though the built-in node policy is host-scoped, so an injected policy never has to recover
/// it by splitting serialized URI text.
#[derive(Debug, Clone)]
pub struct RequestAuthority {
    host: Host,
    port: Option<u16>,
}

impl RequestAuthority {
    /// Extract the authority of a SIP/SIPS URI. Opaque URI schemes have no registrar authority.
    #[must_use]
    pub fn from_uri(uri: &Uri) -> Option<Self> {
        if !uri.scheme().is_sip() {
            return None;
        }
        Some(Self {
            host: uri.host()?.clone(),
            port: uri.port(),
        })
    }

    /// The kernel-parsed host.
    #[must_use]
    pub fn host(&self) -> &Host {
        &self.host
    }

    /// The explicit port, if the URI carried one.
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        self.port
    }
}

/// The two policy decisions at the REGISTER admission boundary.
///
/// One implementation is injected per tenant. Both methods are pure: durable lookups, credential
/// IO and clocks belong outside this interface.
pub trait RegistrationPolicy: std::fmt::Debug + Send + Sync {
    /// S1/S5: whether this tenant serves the URI authority.
    fn serves(&self, tenant: &str, authority: &RequestAuthority) -> bool;

    /// S4: whether the authenticated principal may write the canonical `AoR`.
    ///
    /// `None` is an open tenant's explicit input, not a missing call.
    fn authorizes(&self, tenant: &str, principal: Option<&[u8]>, aor: &CanonicalAor) -> bool;
}

/// Principal-to-AoR grants for one tenant.
///
/// `Open` permits only `principal: None`. `Restricted` permits only byte-exact grants. A principal
/// may receive many `AoRs` and many principals may receive one `AoR`, so aliases, administrators and
/// shared lines are data rather than hard-coded username rules.
#[derive(Debug, Clone, Default)]
pub enum RegistrationAuthorizations {
    /// An explicitly open tenant.
    #[default]
    Open,
    /// An authenticated tenant with an explicit grant set.
    Restricted(Vec<RegistrationGrant>),
}

/// One byte-exact authorization grant.
#[derive(Debug, Clone)]
pub struct RegistrationGrant {
    principal: Bytes,
    aor: CanonicalAor,
}

impl RegistrationAuthorizations {
    /// An explicitly open tenant. Only `principal: None` is authorized.
    #[must_use]
    pub fn open() -> Self {
        Self::Open
    }

    /// A fail-closed authenticated tenant with no grants yet.
    #[must_use]
    pub fn restricted() -> Self {
        Self::Restricted(Vec::new())
    }

    /// Add one principal-to-AoR grant.
    #[must_use]
    pub fn allow(mut self, principal: impl Into<Bytes>, aor: CanonicalAor) -> Self {
        let grant = RegistrationGrant {
            principal: principal.into(),
            aor,
        };
        match &mut self {
            Self::Open => self = Self::Restricted(vec![grant]),
            Self::Restricted(grants) => grants.push(grant),
        }
        self
    }

    /// Decide one S4 input.
    #[must_use]
    pub fn authorizes(&self, principal: Option<&[u8]>, aor: &CanonicalAor) -> bool {
        match self {
            Self::Open => principal.is_none(),
            Self::Restricted(grants) => principal.is_some_and(|principal| {
                grants
                    .iter()
                    .any(|grant| grant.principal.as_ref() == principal && &grant.aor == aor)
            }),
        }
    }
}

/// Explicit policy for an open, serve-any tenant.
///
/// Harnesses that do not model deployment domains still make the S4 decision rather than bypassing
/// it by calling a lower-level parser.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenRegistrationPolicy;

impl RegistrationPolicy for OpenRegistrationPolicy {
    fn serves(&self, _tenant: &str, _authority: &RequestAuthority) -> bool {
        true
    }

    fn authorizes(&self, _tenant: &str, principal: Option<&[u8]>, _aor: &CanonicalAor) -> bool {
        principal.is_none()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn adding_a_grant_makes_even_the_default_policy_restricted() {
        let aor = CanonicalAor::parse(Bytes::from_static(b"sip:line@example.test")).unwrap();
        let policy = RegistrationAuthorizations::default()
            .allow(Bytes::from_static(b"t1:alice"), aor.clone());

        assert!(policy.authorizes(Some(b"t1:alice"), &aor));
        assert!(!policy.authorizes(None, &aor));
        assert!(!policy.authorizes(Some(b"t1:bob"), &aor));
    }
}
