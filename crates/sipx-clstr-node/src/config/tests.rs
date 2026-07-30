//! Tests for the cluster configuration loader.
//!
//! Named after the rules they prove, so a failure names the rule rather than the function.

use super::*;

fn env() -> BTreeMap<String, String> {
    BTreeMap::new()
}

fn env_with(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn identity(node: u16, zone: &str, roles: &[Role]) -> NodeIdentity {
    NodeIdentity {
        node,
        zone: zone.to_owned(),
        roles: roles.iter().copied().collect(),
    }
}

/// A document that loads, so each test below can break exactly one thing.
fn good() -> String {
    r"
apiVersion: sipx.dev/v1alpha1
version: 42
cluster:
  name: acme
  environment: dev
  zones: [a, b, c]
  listener:
    - roles: [edge, registrar]
      transport: udp
      bind: 0.0.0.0:5060
      advertise: 203.0.113.10:5060
  membership:
    - node: 1
      name: node-a
      zone: a
      roles: [edge, registrar]
  locationStore:
    backend: postgres
    dsnRef: location-dsn
  tenant:
    - name: default
      id: 1
      domains: [acme.example]
"
    .to_owned()
}

fn rules(errors: &[ConfigError]) -> Vec<String> {
    errors.iter().map(|e| e.rule.to_string()).collect()
}

#[test]
fn a_well_formed_document_loads() {
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(good().as_bytes(), &who, &env()).expect("should load");
    assert_eq!(config.version, 42);
    assert_eq!(config.name, "acme");
    assert_eq!(config.zones, vec!["a", "b", "c"]);
    assert_eq!(config.tenants.len(), 1);
    // V3: defaults are adopted from the owning spec, not restated differently.
    assert_eq!(config.security.max_forwards, MAX_FORWARDS);
    assert_eq!(config.timers.timer_c_ms, DEFAULT_TIMER_C_MS);
}

/// §2 D3 — JSON is the same data model, read by the same parser.
#[test]
fn cc_d3_json_and_yaml_produce_the_same_config() {
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let from_yaml = load(good().as_bytes(), &who, &env()).expect("yaml loads");
    let json = r#"
{"apiVersion":"sipx.dev/v1alpha1","version":42,"cluster":{
  "name":"acme","environment":"dev","zones":["a","b","c"],
  "listener":[{"roles":["edge","registrar"],"transport":"udp",
               "bind":"0.0.0.0:5060","advertise":"203.0.113.10:5060"}],
  "membership":[{"node":1,"name":"node-a","zone":"a","roles":["edge","registrar"]}],
  "locationStore":{"backend":"postgres","dsnRef":"location-dsn"},
  "tenant":[{"name":"default","id":1,"domains":["acme.example"]}]}}
"#;
    let from_json = load(json.as_bytes(), &who, &env()).expect("json loads");
    assert_eq!(from_yaml, from_json);
}

/// §8 V1 — **the failing-first test for this story.** Every error, ordered by path, not the first.
#[test]
fn cc_v1_reports_every_error_ordered_by_path() {
    let document = r"
apiVersion: sipx.dev/v1alpha1
version: 1
cluster:
  name: acme
  environment: dev
  zones: [a]
  nonsenseKey: 1
  listener:
    - roles: [edge, not-a-role]
      transport: udp
      bind: 0.0.0.0:5060
  membership:
    - node: 0
      name: node-a
      zone: a
      roles: [edge]
  tenant:
    - name: default
      id: 0
      domains: []
";
    let who = identity(1, "a", &[Role::Edge]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");

    // Four independent faults, and all four come back — not just the first.
    assert!(
        errors.len() >= 4,
        "expected at least four errors, got {errors:#?}"
    );
    let found = rules(&errors);
    for rule in ["CC-V2", "CC-R1", "CC-I2"] {
        assert!(
            found.iter().any(|r| r == rule),
            "missing {rule} in {found:?}"
        );
    }

    // Ordered by path, so two runs over one document print the same bytes.
    let paths: Vec<String> = errors.iter().map(|e| e.path.to_string()).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "errors must be ordered by path");

    let again = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    assert_eq!(
        errors, again,
        "two runs over one document must agree exactly"
    );
}

/// §8 V2 — closed world. A typo is an error, and the refusal names what was recognised.
#[test]
fn cc_v2_an_unknown_key_is_an_error_not_a_warning() {
    let document = good().replace("  name: acme", "  name: acme\n  maxContact: 3");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    let error = errors
        .iter()
        .find(|e| e.rule.to_string() == "CC-V2")
        .expect("a closed-world error");
    assert_eq!(error.path.to_string(), "cluster.maxContact");
    assert!(
        error.expected.contains("locationStore"),
        "should name the recognised keys"
    );
}

/// §7 — configuration this build accepts and does not apply is reported **by path**.
///
/// The property under test is the depth, not the existence of a list. `Config::unapplied` used to be a
/// set of top-level section names, and the keys that matter are not top level: `tenant[].auth` and
/// `listener[].tls` are security-relevant and a section-name set cannot name either. A release shipped
/// four silently-discarded security keys with the detector already written.
#[test]
fn fc2_unapplied_configuration_is_reported_by_path_not_by_section() {
    let document = good()
        .replace(
            "      advertise: 203.0.113.10:5060\n",
            "      advertise: 203.0.113.10:5060\n      tls: { certRef: some-cert }\n",
        )
        .replace(
            "      domains: [acme.example]\n",
            "      domains: [acme.example]\n      auth: { realm: acme, secretRef: nonce-key }\n",
        )
        + "  registrar:\n    usePath: true\n";

    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config =
        load(document.as_bytes(), &who, &env()).expect("these keys are accepted, not refused");

    let paths: Vec<String> = config.unapplied.iter().map(ToString::to_string).collect();

    // `auth` is applied since FC-3, so it is NOT here; `tls` is still ignored and must be nameable
    // at the depth it lives at — the property this field exists for.
    assert!(
        paths.iter().any(|p| p == "cluster.listener[0].tls"),
        "an accepted-and-ignored tls block must be nameable: {paths:?}"
    );
    assert!(
        !paths.iter().any(|p| p == "cluster.tenant[0].auth"),
        "auth is applied now, so it must NOT be reported as ignored: {paths:?}"
    );
    // And a top-level section still is.
    assert!(paths.iter().any(|p| p == "cluster.registrar"), "{paths:?}");
}

/// A document that asks for nothing this build ignores reports nothing.
#[test]
fn fc2_a_fully_applied_document_reports_no_unapplied_paths() {
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(good().as_bytes(), &who, &env()).expect("loads");
    assert!(
        config.unapplied.is_empty(),
        "nothing in the baseline document is ignored, so nothing should be reported: {:?}",
        config
            .unapplied
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
}

/// §4 R1 — the role set is closed, and the refusal spells the whole set.
#[test]
fn cc_r1_a_role_outside_the_closed_set_is_refused() {
    let document = good().replace(
        "roles: [edge, registrar]\n      transport",
        "roles: [edge, sbc]\n      transport",
    );
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    let error = errors
        .iter()
        .find(|e| e.rule.to_string() == "CC-R1")
        .expect("a closed-set error");
    assert_eq!(error.found.as_deref(), Some("sbc"));
    assert!(
        error.expected.contains("outbound-proxy"),
        "must spell the closed set"
    );
}

/// §4 R4 — a node that runs nothing should not have been started.
#[test]
fn cc_r4_the_empty_role_set_is_refused() {
    let who = identity(1, "a", &[]);
    let errors = load(good().as_bytes(), &who, &env()).expect_err("must refuse");
    assert!(rules(&errors).contains(&"CC-R4".to_owned()), "{errors:#?}");
}

/// §4 R6 — `echo` runs no proxy role, and `e2e-tester` is refused for the same reason in reverse.
#[test]
fn cc_r6_echo_is_refused_beside_a_call_path_role() {
    let who = identity(1, "a", &[Role::Echo, Role::Edge]);
    let errors = load(good().as_bytes(), &who, &env()).expect_err("must refuse");
    assert!(rules(&errors).contains(&"CC-R6".to_owned()), "{errors:#?}");
}

#[test]
fn cc_r6_e2e_tester_with_echo_is_permitted() {
    // Caller and callee of the same synthetic call, both off the call path.
    let roles: BTreeSet<Role> = [Role::E2eTester, Role::Echo].into_iter().collect();
    let mut errors = Vec::new();
    check_role_combination(&roles, &Path::root(), &mut errors);
    assert!(errors.is_empty(), "{errors:#?}");
}

/// §6 I2 — `0` is reserved, and a duplicate names both holders.
#[test]
fn cc_i2_ids_are_unique_and_zero_is_reserved() {
    let document = good().replace(
        "    - node: 1\n      name: node-a\n      zone: a\n      roles: [edge, registrar]\n",
        "    - node: 1\n      name: node-a\n      zone: a\n      roles: [edge, registrar]\n    \
         - node: 1\n      name: node-b\n      zone: b\n      roles: [edge]\n",
    );
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    let error = errors
        .iter()
        .find(|e| e.rule.to_string() == "CC-I2")
        .expect("a duplicate-id error");
    assert!(
        error.expected.contains("node-a"),
        "must name the other holder: {error}"
    );
}

/// §8 V6 — where an RFC fixes a value, the schema offers no knob.
#[test]
fn cc_v6_max_forwards_is_not_a_knob() {
    let document = good() + "  security:\n    maxForwards: 10\n";
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    let error = errors
        .iter()
        .find(|e| e.rule.to_string() == "CC-V6")
        .expect("a V6 error");
    assert!(
        error.expected.contains("70"),
        "must state the fixed value: {error}"
    );
}

/// §8 V7 — Timer C must exceed three minutes, and it is not `maxCallDuration`.
#[test]
fn cc_v7_timer_c_must_exceed_three_minutes() {
    let document = good() + "  timers:\n    timerC: 60000\n";
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    assert!(rules(&errors).contains(&"CC-V7".to_owned()), "{errors:#?}");
}

/// §8 V9 — no secret value in the document, only a reference.
#[test]
fn cc_v9_an_inline_dsn_is_refused() {
    let document = good().replace("dsnRef: location-dsn", "dsn: postgres://user:pw@db/loc");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    assert!(rules(&errors).contains(&"CC-V9".to_owned()), "{errors:#?}");
}

/// §8 V4 — `${NAME}` resolves from the argument, and an undefined name is an error rather than "".
#[test]
fn cc_v4_substitution_comes_from_the_env_argument() {
    let document = good().replace("advertise: 203.0.113.10:5060", "advertise: ${NODE_IP}:5060");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);

    let config = load(
        document.as_bytes(),
        &who,
        &env_with(&[("NODE_IP", "198.51.100.7")]),
    )
    .expect("resolves");
    assert_eq!(
        config
            .listeners
            .first()
            .expect("one listener")
            .advertise
            .as_deref(),
        Some("198.51.100.7:5060")
    );

    // Undefined must name the variable, not silently become the empty string — otherwise the error
    // that surfaces is an unparsable address, which is the wrong problem.
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    let error = errors
        .iter()
        .find(|e| e.rule.to_string() == "CC-V4")
        .expect("a V4 error");
    assert_eq!(error.found.as_deref(), Some("${NODE_IP}"));
}

/// §8 V4 — the substitution grammar is exactly `[A-Z_][A-Z0-9_]*`; nothing else is a variable.
#[test]
fn cc_v4_only_upper_snake_names_are_variables() {
    assert!(is_var_name("NODE_IP"));
    assert!(is_var_name("_X9"));
    assert!(!is_var_name("node_ip"));
    assert!(!is_var_name("9X"));
    assert!(!is_var_name(""));
}

/// §5 P3 — the document's membership entry is cross-checked against the identity, not obeyed.
#[test]
fn cc_p3_a_membership_mismatch_names_both_sides() {
    let who = identity(1, "b", &[Role::Edge, Role::Registrar]); // document says zone a
    let errors = load(good().as_bytes(), &who, &env()).expect_err("must refuse");
    let error = errors
        .iter()
        .find(|e| e.rule.to_string() == "CC-P3")
        .expect("a P3 error");
    assert!(
        error.found.as_deref().unwrap().contains('a'),
        "names the document's value"
    );
    assert!(error.expected.contains('b'), "names the identity's value");
}

/// §5 P3 — a node with *no* entry still starts. The operator may not have published it yet.
#[test]
fn cc_p3_an_absent_membership_entry_is_not_an_error() {
    let who = identity(7, "a", &[Role::Edge, Role::Registrar]);
    load(good().as_bytes(), &who, &env()).expect("a node the document does not list still loads");
}

/// §2 D2 / §5 P1 / §4 R3 — same bytes, two identities, two different configs, and the loader never
/// branches on role to get there.
#[test]
fn cc_d2_one_document_projects_differently_per_identity() {
    // Two listeners and two members, so the same bytes mean different things to different nodes.
    let document = r"
apiVersion: sipx.dev/v1alpha1
version: 42
cluster:
  name: acme
  environment: dev
  zones: [a]
  listener:
    - roles: [registrar]
      transport: udp
      bind: 0.0.0.0:5060
      advertise: 203.0.113.10:5060
    - roles: [e2e-tester]
      transport: udp
      bind: 0.0.0.0:5062
      advertise: 203.0.113.10:5062
  membership:
    - node: 1
      name: node-a
      zone: a
      roles: [registrar]
    - node: 2
      name: node-b
      zone: a
      roles: [e2e-tester]
  locationStore:
    backend: postgres
    dsnRef: location-dsn
  tenant:
    - name: default
      id: 1
      domains: [acme.example]
";

    let registrar = identity(1, "a", &[Role::Registrar]);
    let prober = identity(2, "a", &[Role::E2eTester]);

    let as_registrar = {
        let who = registrar.clone();
        let config = load(document.as_bytes(), &who, &env()).expect("loads for the registrar");
        project(&config, &who)
    };
    let as_prober = {
        let config = load(document.as_bytes(), &prober, &env()).expect("loads for the prober");
        project(&config, &prober)
    };

    assert_eq!(as_registrar.listeners.len(), 1);
    let registrar_listener = as_registrar.listeners.first().expect("one listener");
    assert_eq!(registrar_listener.bind, "0.0.0.0:5060");
    assert_eq!(as_prober.listeners.len(), 1);
    let prober_listener = as_prober.listeners.first().expect("one listener");
    assert_eq!(prober_listener.bind, "0.0.0.0:5062");
    assert_ne!(as_registrar.listeners, as_prober.listeners);

    // R5: the location store is the registrar's section, so it is projected away for a node that
    // does not run that role — not merely ignored by it.
    assert!(as_registrar.location_store.is_some());
    assert!(as_prober.location_store.is_none());
}

/// §5 P4 — a projected node must hold at least one listener.
#[test]
fn cc_p4_a_node_with_no_listener_of_its_own_is_refused() {
    let who = identity(1, "a", &[Role::OutboundProxy]);
    let errors = load(good().as_bytes(), &who, &env()).expect_err("must refuse");
    assert!(rules(&errors).contains(&"CC-P4".to_owned()), "{errors:#?}");
}

/// §3 — the schema version is checked, because a document written for another schema is not this one.
#[test]
fn a_foreign_api_version_is_refused() {
    let document = good().replace("sipx.dev/v1alpha1", "sipx.dev/v2");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    let error = errors
        .iter()
        .find(|e| e.rule.to_string() == "CC-3")
        .expect("a version error");
    assert_eq!(error.expected, API_VERSION);
}

/// §8 V8 — declared ceilings are checked at load. Raising one is a spec change, not a flag.
#[test]
fn cc_v8_the_zone_ceiling_is_checked() {
    let many: Vec<String> = (0..65).map(|i| format!("z{i}")).collect();
    let document = good().replace("zones: [a, b, c]", &format!("zones: [{}]", many.join(", ")));
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    assert!(rules(&errors).contains(&"CC-V8".to_owned()), "{errors:#?}");
}

/// §8 V10 — refusing is total. A refused document yields no config at all, not a partial one.
#[test]
fn cc_v10_a_refusal_yields_no_config() {
    let document = good().replace("  name: acme\n", "");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let result = load(document.as_bytes(), &who, &env());
    assert!(
        result.is_err(),
        "a document missing a required field must be refused outright"
    );
}

/// §6 I4 — names are matched byte-for-byte. No folding would make two tenants one.
#[test]
fn cc_i4_names_are_not_folded() {
    let document = good().replace(
        "    - name: default\n      id: 1\n      domains: [acme.example]\n",
        "    - name: default\n      id: 1\n      domains: []\n    - name: DEFAULT\n      id: 2\n      domains: []\n",
    );
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(document.as_bytes(), &who, &env())
        .expect("`default` and `DEFAULT` are two tenants, not one");
    assert_eq!(config.tenants.len(), 2);
}

/// §2 D1 — the loader is pure: the same inputs give the same answer, and nothing outside them is
/// consulted. Asserted by construction here; the layering test asserts the absence of a runtime.
#[test]
fn cc_d1_load_is_deterministic_in_its_inputs() {
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let first = load(good().as_bytes(), &who, &env());
    let second = load(good().as_bytes(), &who, &env());
    assert_eq!(first, second);
}

/// §2 D3 — **TOML is the third encoding, and it must produce the identical `Config`.**
///
/// The rule is about the typed tree, not the spelling, so the assertion is equality of the whole
/// value rather than a spot check on one field. A converter that dropped or retyped anything would
/// show up here and nowhere else.
#[test]
fn cc_d3_toml_produces_the_same_config_as_yaml() {
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let from_yaml = load(good().as_bytes(), &who, &env()).expect("yaml loads");

    let toml_document = r#"
apiVersion = "sipx.dev/v1alpha1"
version = 42

[cluster]
name = "acme"
environment = "dev"
zones = ["a", "b", "c"]

[[cluster.listener]]
roles = ["edge", "registrar"]
transport = "udp"
bind = "0.0.0.0:5060"
advertise = "203.0.113.10:5060"

[[cluster.membership]]
node = 1
name = "node-a"
zone = "a"
roles = ["edge", "registrar"]

[cluster.locationStore]
backend = "postgres"
dsnRef = "location-dsn"

[[cluster.tenant]]
name = "default"
id = 1
domains = ["acme.example"]
"#;
    let from_toml = load(toml_document.as_bytes(), &who, &env()).expect("toml loads");
    assert_eq!(
        from_yaml, from_toml,
        "the encoding must not change the config"
    );
}

/// The encoding is detected from the bytes, so the closed world still closes in TOML.
#[test]
fn cc_v2_a_typo_in_a_toml_document_is_still_an_error() {
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let document = r#"
apiVersion = "sipx.dev/v1alpha1"
version = 1

[cluster]
name = "acme"
environment = "dev"
zones = ["a"]
maxContact = 3

[[cluster.listener]]
roles = ["edge", "registrar"]
transport = "udp"
bind = "0.0.0.0:5060"
advertise = "203.0.113.10:5060"
"#;
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    let error = errors
        .iter()
        .find(|e| e.rule.to_string() == "CC-V2")
        .expect("a closed-world error");
    assert_eq!(error.path.to_string(), "cluster.maxContact");
}

/// Neither valid TOML nor valid YAML is refused as a document, not as a mystery.
#[test]
fn cc_d3_something_that_is_neither_encoding_is_refused() {
    let who = identity(1, "a", &[Role::Edge]);
    let errors = load(
        b"[unterminated
  = = =",
        &who,
        &env(),
    )
    .expect_err("must refuse");
    assert!(
        errors.iter().any(|e| e.rule.to_string() == "CC-D3"),
        "{errors:#?}"
    );
}

/// §8 V10 — a transport this build cannot serve is **refused**, never downgraded.
///
/// The regression this pins: `transport: tls` used to fall through a `_ => Udp` default, so a document
/// asking for encrypted signalling produced a node answering `200 OK` in plaintext, with nothing
/// anywhere saying so. The operator's intent was in the document and was discarded.
#[test]
fn cc_v10_a_tls_listener_is_refused_not_downgraded_to_cleartext() {
    let document = good().replace("transport: udp", "transport: tls");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    let error = errors
        .iter()
        .find(|e| e.path.to_string() == "cluster.listener[0].transport")
        .expect("a transport error");
    assert_eq!(error.found.as_deref(), Some("tls"));
    assert!(
        error.expected.contains("substitute cleartext"),
        "the refusal must say it will not downgrade: {error}"
    );
}

/// A transport that does not exist at all is a different problem, and says so.
#[test]
fn cc_v2_an_unknown_transport_names_the_closed_set() {
    let document = good().replace("transport: udp", "transport: sctp");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    let error = errors
        .iter()
        .find(|e| e.path.to_string() == "cluster.listener[0].transport")
        .expect("a transport error");
    assert!(
        error.expected.contains("wss"),
        "must spell the set: {error}"
    );
}

/// **`FC-4`.** `tenant[]`'s policy keys reach the registrar instead of being dropped.
///
/// Before this story `maxBindingsPerAor: 3` loaded clean and the effective cap stayed 10, because the
/// driver built `TenantPolicy::default()` regardless of the document.
#[test]
fn fc4_tenant_policy_is_read_from_the_document() {
    let document = good().replace(
        "      domains: [acme.example]\n",
        "      domains: [acme.example]\n      maxBindingsPerAor: 3\n      expiry: { default: 600, min: 30, max: 1200 }\n",
    );
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(document.as_bytes(), &who, &env()).expect("loads");
    let tenant = config.tenants.first().expect("one tenant");

    assert_eq!(tenant.policy.max_bindings_per_aor, 3);
    assert_eq!(tenant.policy.default_expires, 600);
    assert_eq!(tenant.policy.min_expires, 30);
    assert_eq!(tenant.policy.max_expires, 1_200);

    // And none of it is reported as ignored — FC-2's warning must not lie in either direction.
    let paths: Vec<String> = config.unapplied.iter().map(ToString::to_string).collect();
    assert!(
        !paths
            .iter()
            .any(|p| p.contains("maxBindingsPerAor") || p.contains("expiry")),
        "applied keys must not be warned about: {paths:?}"
    );
}

/// A document that says nothing keeps location-service's own defaults, not a second set.
#[test]
fn fc4_absent_policy_keys_keep_the_specs_defaults() {
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(good().as_bytes(), &who, &env()).expect("loads");
    let policy = &config.tenants.first().expect("one tenant").policy;
    assert_eq!(policy.default_expires, 3_600);
    assert_eq!(policy.min_expires, 60);
    assert_eq!(policy.max_expires, 86_400);
    assert_eq!(policy.max_bindings_per_aor, 10);
}

/// A minimum above the maximum is refused, not silently reordered — which of the two the operator
/// meant is not this schema's to guess.
#[test]
fn fc4_an_inverted_expiry_range_is_refused() {
    let document = good().replace(
        "      domains: [acme.example]\n",
        "      domains: [acme.example]\n      expiry: { min: 9000, max: 60 }\n",
    );
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    assert!(
        errors.iter().any(|e| e.path.to_string().contains("expiry")),
        "{errors:#?}"
    );
}

/// A quota of zero is a disabled tenant spelled as a limit, and is refused as such.
#[test]
fn fc4_a_zero_quota_is_refused() {
    let document = good().replace(
        "      domains: [acme.example]\n",
        "      domains: [acme.example]\n      maxBindingsPerAor: 0\n",
    );
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    assert!(
        errors
            .iter()
            .any(|e| e.path.to_string().contains("maxBindingsPerAor")),
        "{errors:#?}"
    );
}
// ------------------------------------------------------------------ DP-11: the admission bound ---

/// The bound is configuration, not a constant.
#[test]
fn dp11_the_admission_bound_is_read_from_the_document() {
    let document = with_admission("maxInFlightTransactions: 64");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(document.as_bytes(), &who, &env()).expect("should load");
    assert_eq!(config.admission.max_in_flight_transactions, 64);

    // And it survives projection onto the node, which is the only shape the driver ever sees.
    let projected = project(&config, &who);
    assert_eq!(projected.admission.max_in_flight_transactions, 64);
}

/// A document that says nothing about overload gets the declared default — not zero, and not "no
/// bound", which the schema deliberately cannot spell.
#[test]
fn dp11_an_absent_admission_section_is_the_declared_default() {
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(good().as_bytes(), &who, &env()).expect("should load");
    assert_eq!(
        config.admission.max_in_flight_transactions,
        DEFAULT_MAX_IN_FLIGHT_TRANSACTIONS
    );
}

/// The bound is **applied**, so it must not be reported as ignored — `FC-2`'s warning would then lie
/// in the other direction, which is the mistake that story's own note calls out.
#[test]
fn dp11_the_admission_section_is_not_reported_as_unapplied() {
    let document = with_admission("maxInFlightTransactions: 16");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(document.as_bytes(), &who, &env()).expect("should load");
    let paths: Vec<String> = config.unapplied.iter().map(ToString::to_string).collect();
    assert!(
        !paths.iter().any(|path| path.contains("admission")),
        "a key this build applies must not be listed as unapplied: {paths:?}"
    );
}

/// Zero is refused. It is not a smaller limit, it is a node that answers `503` to every call, and
/// §8 V10's posture is to refuse a configuration rather than to honour something else.
#[test]
fn dp11_an_admission_bound_of_zero_is_refused() {
    let document = with_admission("maxInFlightTransactions: 0");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    let error = errors
        .iter()
        .find(|e| e.path.to_string() == "cluster.admission.maxInFlightTransactions")
        .expect("an admission error");
    assert_eq!(error.rule.to_string(), "CC-V8");
    assert_eq!(error.found.as_deref(), Some("0"));
}

/// §8 V2 still holds one level down: a typo inside the section is an error, not a silent default.
#[test]
fn cc_v2_an_unknown_key_under_admission_is_refused() {
    let document = with_admission("maxInFlightTransaction: 16");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    assert!(
        errors.iter().any(|e| e.rule.to_string() == "CC-V2"
            && e.path.to_string() == "cluster.admission.maxInFlightTransaction"),
        "a near-miss key must be refused: {errors:#?}"
    );
}

/// The good document with an `admission` section carrying one line.
fn with_admission(line: &str) -> String {
    good().replace(
        "  zones: [a, b, c]",
        &format!("  zones: [a, b, c]\n  admission:\n    {line}"),
    )
}

// ------------------------------------------- DP-12: Timer C's default, and the keys nobody reads ---

/// The good document with a `timers` section carrying the given already-indented lines.
fn with_timers(lines: &str) -> String {
    good().replace(
        "  zones: [a, b, c]",
        &format!("  zones: [a, b, c]\n  timers:\n{lines}"),
    )
}

/// §12 `CC-V-12` — a `timers` section that names some timers but **not** `timerC` loads, and Timer C
/// takes §8 V7's declared default.
///
/// This is the whole of `DP-12` in one document. V7 used to declare the default as exactly 180 s
/// beside a rule requiring *more* than 3 minutes (RFC 3261 §16.6 step 11), so the loader refused the
/// value the loader itself had supplied, and the only way to carry a `timers` section at all was to
/// spell `timerC` out. The literal below is the spec's number on purpose: moving one without moving
/// the other is the drift this row exists to catch.
#[test]
fn cc_v_12_a_timers_section_without_timer_c_loads_with_the_declared_default() {
    let document = with_timers("    t1: 500\n");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(document.as_bytes(), &who, &env()).expect("should load");
    assert_eq!(config.timers.t1_ms, 500);
    assert_eq!(
        config.timers.timer_c_ms, 240_000,
        "§8 V7's declared default"
    );
    assert!(
        config.timers.timer_c_ms > 180_000,
        "a default that cannot satisfy the rule declared beside it is the defect DP-12 closed"
    );
}

/// A document with no `timers` section at all lands on the same compliant default.
///
/// It reached one before this story too — but only because the floor was never checked on the
/// defaulted path, which is precisely what hid the contradiction. The check now runs against
/// whatever value stands, written or defaulted.
#[test]
fn dp12_a_document_with_no_timers_section_carries_a_compliant_timer_c() {
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(good().as_bytes(), &who, &env()).expect("should load");
    assert_eq!(config.timers.timer_c_ms, 240_000);
}

/// §12 `CC-V-9` — `timerC` below the floor is refused, naming the path and the rule.
#[test]
fn cc_v_9_a_timer_c_below_three_minutes_is_refused() {
    let document = with_timers("    timerC: 120000\n");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    let error = errors
        .iter()
        .find(|e| e.path.to_string() == "cluster.timers.timerC")
        .expect("a timerC error");
    assert_eq!(error.rule.to_string(), "CC-V7");
    assert_eq!(error.found.as_deref(), Some("120000 ms"));
}

/// The bound stayed **exclusive**, which is the half of `DP-12` that could have been settled the
/// other way and was not.
///
/// RFC 3261 §16.6 step 11 says Timer C "MUST be larger than 3 minutes" — a MUST over a strict
/// inequality, with no SHOULD and no rounding language anywhere near it. Relaxing the loader to `>=`
/// would have made the old default legal at the cost of admitting a value the RFC forbids, so the
/// default moved instead. Exactly three minutes is still a refusal.
#[test]
fn dp12_exactly_three_minutes_is_still_refused() {
    let document = with_timers("    timerC: 180000\n");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    assert!(
        errors
            .iter()
            .any(|e| e.path.to_string() == "cluster.timers.timerC"
                && e.rule.to_string() == "CC-V7"),
        "the floor is exclusive, per RFC 3261 §16.6 step 11: {errors:#?}"
    );
}

/// One millisecond over the floor is accepted. The rule is the RFC's, not a wider one this loader
/// invented while fixing the default.
#[test]
fn dp12_one_millisecond_over_the_floor_is_accepted() {
    let document = with_timers("    timerC: 180001\n");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(document.as_bytes(), &who, &env()).expect("should load");
    assert_eq!(config.timers.timer_c_ms, 180_001);
}

/// **`FC-2`.** The keys §7 declares that nothing in this build reads are *reported*, not dropped.
///
/// `timers.maxCallDuration`, `locationStore.ha` and `listener[].tls` are all on the closed-world
/// allow-lists, so a document may carry them — but no field of `Config` holds them and no driver
/// consults them. Accepted-and-silently-discarded is the class `FC-2` added `unapplied` to
/// eliminate; an operator who sets a session cap today gets nothing and is told nothing.
#[test]
fn dp12_recognised_but_unread_keys_are_reported_as_unapplied() {
    let document = good()
        .replace(
            "      advertise: 203.0.113.10:5060\n",
            "      advertise: 203.0.113.10:5060\n      tls: { certRef: edge-cert }\n",
        )
        .replace(
            "    dsnRef: location-dsn\n",
            "    dsnRef: location-dsn\n    ha: true\n",
        )
        .replace(
            "  zones: [a, b, c]",
            "  zones: [a, b, c]\n  timers:\n    maxCallDuration: 28800000",
        );
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(document.as_bytes(), &who, &env()).expect("should load");
    let paths: Vec<String> = config.unapplied.iter().map(ToString::to_string).collect();
    for expected in [
        "cluster.timers.maxCallDuration",
        "cluster.locationStore.ha",
        "cluster.listener[0].tls",
    ] {
        assert!(
            paths.iter().any(|path| path == expected),
            "{expected} is accepted and read by nothing, so it must be reported: {paths:?}"
        );
    }
}
