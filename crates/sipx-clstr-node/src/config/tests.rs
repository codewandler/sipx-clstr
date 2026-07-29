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
    assert_eq!(config.timers.timer_c_ms, 180_000);
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

/// §7 — a deferred section is recognised, reported, and not silently dropped.
#[test]
fn deferred_sections_are_recognised_and_reported() {
    let document = good() + "  observability:\n    metrics: true\n  probe:\n    schedule: 30s\n";
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(document.as_bytes(), &who, &env()).expect("deferred sections are legal");
    assert!(config.deferred.contains("observability"));
    assert!(config.deferred.contains("probe"));
    assert!(
        !config.deferred.contains("trunk"),
        "absent sections are not deferred"
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
fn cc_3_a_foreign_api_version_is_refused() {
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
