//! What a refusal says.
//!
//! [`ConfigError`] is the shape [cluster-config](../../../../docs/specs/cluster-config.md) §8 fixes:
//! the document's own spelling of the path, the rule that rejected it, what was found and what was
//! expected. Nothing here is a message string assembled at the point of failure — a caller that
//! wants to group by rule, or a test that wants to assert on one, needs the parts.

use std::fmt;

/// A path into the document, in the document's own spelling — `cluster.trunk[2].media.srtp`.
///
/// Built by pushing as the walk descends, so a path is always the real location rather than a
/// reconstruction. Ordering is lexicographic over the rendered form, which is what gives §8 V1 its
/// byte-identical output across runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Path(String);

impl Path {
    /// The document root.
    #[must_use]
    pub fn root() -> Self {
        Self(String::new())
    }

    /// `self.key`.
    #[must_use]
    pub fn field(&self, key: &str) -> Self {
        if self.0.is_empty() {
            Self(key.to_owned())
        } else {
            Self(format!("{}.{key}", self.0))
        }
    }

    /// `self[index]`.
    #[must_use]
    pub fn index(&self, index: usize) -> Self {
        Self(format!("{}[{index}]", self.0))
    }

    /// The last key in the path — `auth` for `cluster.tenant[0].auth`.
    ///
    /// Exists so a caller can ask "is this the auth key?" without a suffix match on the rendered
    /// string. A `ends_with(".auth")` would also match a key genuinely called `foo.auth`, and it reads
    /// to both a human and to clippy as a filename-extension test, which it is not.
    #[must_use]
    pub fn leaf(&self) -> &str {
        self.0.rsplit('.').next().unwrap_or(&self.0)
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            f.write_str("<document>")
        } else {
            f.write_str(&self.0)
        }
    }
}

/// The rule that rejected a document, by the id its owning spec gives it.
///
/// A rule id belongs to whichever spec owns the field — §8 V5 is explicit that a cross-section
/// failure reports the *owner's* id, not this schema's. So this is a string rather than an enum:
/// an enum here would have to grow a variant every time another spec added a rule, and the
/// compiler cannot check the ids anyway.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RuleId(String);

impl RuleId {
    pub fn new(id: &str) -> Self {
        Self(id.to_owned())
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One reason a document was refused.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConfigError {
    /// Where, in the document's spelling.
    pub path: Path,
    /// Which rule, by the owning spec's id.
    pub rule: RuleId,
    /// What was there, when quoting it helps and does not leak a secret.
    ///
    /// On a `CC-V9` path this is a description or `None`, never the inline value: the same display
    /// text is safe to reuse in stderr and in an operator status message.
    pub found: Option<String>,
    /// What would have been accepted.
    pub expected: String,
}

impl ConfigError {
    pub fn new(path: Path, rule: &str, found: Option<String>, expected: &str) -> Self {
        Self {
            path,
            rule: RuleId::new(rule),
            found,
            expected: expected.to_owned(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} [{}]: ", self.path, self.rule)?;
        match &self.found {
            Some(found) => write!(f, "found {found}, expected {}", self.expected),
            None => write!(f, "expected {}", self.expected),
        }
    }
}

/// Sort into the order §8 V1 requires, so two runs over one document print the same bytes.
///
/// By path first, then rule, then the found value: one path can carry more than one failure, and
/// leaving those in discovery order would make the output depend on the walk.
pub fn ordered(mut errors: Vec<ConfigError>) -> Vec<ConfigError> {
    errors.sort();
    errors.dedup();
    errors
}
