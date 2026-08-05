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
///
/// The member carries an `rpc` endpoint because `cluster-membership` MB5 requires one of every
/// member on the call path, and `edge`+`registrar` is exactly that. Before `DP-16` this document
/// could not have carried it: the closed world of a member was `{node, name, zone, roles}`.
fn good() -> String {
    r#"
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
      rpc: "10.0.0.1:7223"
  locationStore:
    backend: postgres
    dsnRef: location-dsn
  tenant:
    - name: default
      id: 1
      domains: [acme.example]
"#
    .to_owned()
}

/// The `unapplied` paths every document in this file carries, whatever else it declares.
///
/// MB5 makes `rpc` mandatory for a member on the call path, and nothing in this build dials one —
/// `AF-3`/`AF-7` own the connection-owner RPC. So the baseline is not empty any more, and saying so
/// once here is what keeps each test below asserting about the key it is actually testing.
fn baseline_unapplied() -> Vec<String> {
    vec!["cluster.membership[0].rpc".to_owned()]
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

/// §12 `CC-D-2` — JSON is the same data model, read by the same parser (§2 D3).
///
/// The assertion is equality of the whole `Config` rather than a spot check on one field, because
/// the rule is about the typed tree and not about the spelling: a converter that dropped or retyped
/// anything would show up here and nowhere else.
#[test]
fn cc_d_2_the_same_tree_in_yaml_and_json_is_one_config() {
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let from_yaml = load(good().as_bytes(), &who, &env()).expect("yaml loads");
    let json = r#"
{"apiVersion":"sipx.dev/v1alpha1","version":42,"cluster":{
  "name":"acme","environment":"dev","zones":["a","b","c"],
  "listener":[{"roles":["edge","registrar"],"transport":"udp",
               "bind":"0.0.0.0:5060","advertise":"203.0.113.10:5060"}],
  "membership":[{"node":1,"name":"node-a","zone":"a","roles":["edge","registrar"],
                 "rpc":"10.0.0.1:7223"}],
  "locationStore":{"backend":"postgres","dsnRef":"location-dsn"},
  "tenant":[{"name":"default","id":1,"domains":["acme.example"]}]}}
"#;
    let from_json = load(json.as_bytes(), &who, &env()).expect("json loads");
    assert_eq!(from_yaml, from_json);
}

/// §12 `CC-D-6` — a document with unrelated mistakes costs one restart, not one per mistake.
///
/// §8 V1: every error, ordered by path, byte-identical across two runs. This was `DP-8`'s own
/// failing-first test and keeps its assertions; what changed in `DP-16` is its name, so the row it
/// has always executed is the row a reader can find it by.
#[test]
fn cc_d_6_reports_every_error_ordered_by_path() {
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

/// A document that asks for nothing this build ignores reports exactly the fields it must.
///
/// This assertion was `unapplied.is_empty()` until `DP-16`, and the change is the story in one line:
/// MB5 makes `rpc` mandatory for every member on the call path, and nothing in this build dials one.
/// A mandatory field with no consumer is still a field with no consumer, so it is reported — the
/// alternative is a list that is quiet about the one key every document now carries, which is `FC-2`'s
/// warning lying in the direction that flatters the node.
#[test]
fn fc2_a_fully_applied_document_reports_only_what_no_consumer_reads() {
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(good().as_bytes(), &who, &env()).expect("loads");
    let paths: Vec<String> = config.unapplied.iter().map(ToString::to_string).collect();
    assert_eq!(paths, baseline_unapplied());
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

/// §12 `CC-I-2` — `0` is reserved for a tenant id, and `affinity-token` §3 spells it "none/system".
#[test]
fn cc_i_2_a_tenant_id_of_zero_is_reserved() {
    let document = good().replace("      id: 1\n", "      id: 0\n");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    let error = errors
        .iter()
        .find(|e| e.path.to_string() == "cluster.tenant[0].id")
        .expect("a reserved-id error");
    assert_eq!(error.path.to_string(), "cluster.tenant[0].id");
    assert_eq!(error.rule.to_string(), "CC-I2");
    assert_eq!(error.found.as_deref(), Some("0"));
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

/// §12 `CC-D-4` — an undefined `${NAME}` is an error naming the variable **and the field it was
/// written in**, never an address error and never the empty string.
///
/// The empty string would turn `advertise: "${NODE_IP}:5060"` into `:5060` and report the wrong
/// problem one layer down; the document root would report the right problem and leave an operator
/// grepping for which of forty fields named it.
#[test]
fn cc_d_4_an_undefined_variable_is_reported_at_the_field_that_named_it() {
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
    assert_eq!(error.path.to_string(), "cluster.listener[0].advertise");
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

/// §12 `CC-R-7` — the document's membership entry is cross-checked against the identity, never
/// obeyed (§5 P3, `cluster-membership` MB2), and a mismatch names both sides.
#[test]
fn cc_r_7_a_membership_zone_mismatch_names_both_sides() {
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

/// §12 `CC-R-11` — a member may declare the fields `cluster-membership` §3 adds.
///
/// **The failing-first test for `DP-16`.** `AF-6` wrote §3 and named no story to implement it, so
/// the closed world of a membership entry stayed `{node, name, zone, roles}` and a document written
/// to the published spec was refused at the merge base by two `CC-V2` errors naming `rpc` and
/// `incarnationSource` — the schema and the loader one story apart, which is what the spec's own §12
/// says and what this story closes.
#[test]
fn cc_r_11_a_member_declares_the_rpc_endpoint_and_incarnation_source_of_section_3() {
    let document = good().replace(
        "      rpc: \"10.0.0.1:7223\"\n",
        "      rpc: \"10.0.0.1:7223\"\n      incarnationSource: boot-second\n",
    );
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(document.as_bytes(), &who, &env())
        .expect("`rpc` and `incarnationSource` are fields of a member (cluster-membership §3)");

    let member = config.membership.first().expect("one member");
    assert_eq!(member.rpc.as_deref(), Some("10.0.0.1:7223"));
    assert_eq!(member.incarnation_source, IncarnationSource::BootSecond);

    // MB8's default is a documented mechanism, not a silence: omitting the field selects
    // `boot-second` and says so.
    let config = load(good().as_bytes(), &who, &env()).expect("loads without the field");
    let member = config.membership.first().expect("one member");
    assert_eq!(member.incarnation_source, IncarnationSource::BootSecond);
    assert_eq!(member.incarnation_ref, None);
}

/// §12 `CC-R-8` — a node with *no* entry still starts (§5 P3, MB2).
///
/// A node whose pod the operator has not yet published would otherwise be unable to start, and the
/// failure would arrive as a crash loop rather than as a mismatch.
#[test]
fn cc_r_8_an_absent_membership_entry_is_not_an_error() {
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
      rpc: '10.0.0.1:7223'
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

/// §12 `CC-D-1` — a document written against another schema is refused, and **nothing else is
/// parsed or reported**.
///
/// §3 D6 is unusually explicit about the second half: "It MUST NOT parse a document it does not
/// fully implement — not on a best-effort basis, not by ignoring what it does not recognise." So
/// this is the one refusal in the loader that is not accumulated beside others. Reporting §8 V1's
/// full list here would answer a question nobody asked — every closed-world complaint would be about
/// a schema this build has no opinion on, which reads as "these keys are wrong" when the true
/// statement is "this whole document belongs to another version".
#[test]
fn cc_d_1_a_foreign_api_version_is_refused_and_nothing_else_is_reported() {
    // Three further faults, every one of which this loader reports when the schema version matches.
    let document = good()
        .replace("sipx.dev/v1alpha1", "sipx.dev/v2")
        .replace("  name: acme", "  name: acme\n  maxContact: 3")
        .replace("transport: udp", "transport: sctp");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");

    assert_eq!(
        errors.len(),
        1,
        "nothing else is parsed or reported: {errors:#?}"
    );
    let error = errors.first().expect("one error");
    assert_eq!(error.path.to_string(), "apiVersion");
    assert_eq!(error.rule.to_string(), "CC-D6");
    assert_eq!(error.found.as_deref(), Some("sipx.dev/v2"));
    assert_eq!(
        error.expected, API_VERSION,
        "it names the version it does implement"
    );
}

/// §12 `CC-D-9` — there is no default configuration version.
#[test]
fn cc_d_9_a_document_with_no_version_is_refused() {
    let document = good().replace("version: 42\n", "");
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    let error = errors
        .iter()
        .find(|e| e.path.to_string() == "version")
        .expect("a version error");
    assert_eq!(error.rule.to_string(), "CC-D9");
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

/// §12 `CC-I-3` — names are matched byte-for-byte (§6 I4). Folding would make two tenants one.
#[test]
fn cc_i_3_names_are_not_folded() {
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
rpc = "10.0.0.1:7223"

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
    let errors = load(b"[unterminated\n\x00\x01 = = =", &who, &env()).expect_err("must refuse");
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

// -------------------------------------------------------- roles, and what they wire (§4, DP-13) ---

/// **CC-R-1.** One binary, four roles, no ambiguity (R2, R3).
///
/// The row is about a document that loads, and the second half of it is the part that had no proof:
/// R3 makes a role select which decision paths are *wired*, so the four call-path roles on one node
/// have to collapse into a wiring rather than into a question a request would have to ask. Deferred
/// to `DP-8` until `DP-13`, which is when there was a wiring to compare it against.
#[test]
fn cc_r_1_four_roles_on_one_node_load_and_wire_both_paths() {
    let document = r"
apiVersion: sipx.dev/v1alpha1
version: 1
cluster:
  name: acme
  environment: dev
  zones: [a]
  listener:
    - roles: [edge, registrar, inbound-proxy, outbound-proxy]
      transport: udp
      bind: 0.0.0.0:5060
      advertise: 203.0.113.10:5060
  membership:
    - node: 1
      name: node-a
      zone: a
      roles: [edge, registrar, inbound-proxy, outbound-proxy]
      rpc: '10.0.0.1:7223'
  locationStore:
    backend: memory
  tenant:
    - name: default
      id: 1
      domains: [acme.example]
";
    let who = identity(
        1,
        "a",
        &[
            Role::Edge,
            Role::Registrar,
            Role::InboundProxy,
            Role::OutboundProxy,
        ],
    );
    let config = load(document.as_bytes(), &who, &env()).expect("four roles on one node load");
    let projected = project(&config, &who);
    assert_eq!(projected.listeners.len(), 1);
    // No ambiguity: `inbound-proxy` and `outbound-proxy` are one forwarding path, not two, and
    // nothing downstream can ask which of them it is.
    assert_eq!(Capabilities::of(&who.roles), Capabilities::CALL_PATH);
}

/// **`DP-13`.** A role wires a path; the roles that wire neither do not silently acquire one.
///
/// The projection used the roles and dropped them, so this mapping did not exist anywhere and the
/// driver dispatched on method alone.
#[test]
fn dp13_capabilities_are_the_union_of_what_the_roles_wire() {
    let wiring = |roles: &[Role]| Capabilities::of(&roles.iter().copied().collect());

    assert_eq!(
        wiring(&[Role::Registrar]),
        Capabilities {
            registrar: true,
            proxy: false
        },
        "a registrar answers REGISTER and forwards nothing (§7)"
    );
    assert_eq!(
        wiring(&[Role::InboundProxy]),
        Capabilities {
            registrar: false,
            proxy: true
        },
        "a proxy carries calls and registers nobody"
    );
    assert_eq!(wiring(&[Role::Edge]), wiring(&[Role::OutboundProxy]));
    assert_eq!(
        wiring(&[Role::Edge, Role::Registrar]),
        Capabilities::CALL_PATH
    );
    // R6 refuses these beside a call-path role, and this build has no path for either: what they
    // must not do is arrive at the driver looking like a proxy.
    assert_eq!(
        wiring(&[Role::Echo]),
        Capabilities {
            registrar: false,
            proxy: false
        }
    );
    assert_eq!(wiring(&[Role::E2eTester]), wiring(&[Role::Echo]));
}

/// A `security` block declaring one of the four unapplied controls, with everything else valid.
fn with_security(body: &str) -> String {
    good().replace(
        "  locationStore:",
        &format!("  security:\n{body}  locationStore:"),
    )
}

/// §12 CC-V-13 — a single declared control is refused, and the refusal describes rather than echoes.
#[test]
fn cc_v_13_a_declared_security_control_is_refused() {
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let document = with_security("    unknownSource: drop\n");
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");

    let found = rules(&errors);
    assert!(
        found.iter().any(|r| r == "CC-V10"),
        "expected CC-V10 for an unapplied control, got {found:?}"
    );
    let about: Vec<String> = errors.iter().map(|e| e.path.to_string()).collect();
    assert!(
        about.iter().any(|p| p == "cluster.security.unknownSource"),
        "the refusal must name the declared path, got {about:?}"
    );
    // V9 / `FC-8`: the message says what the control would decide, never what was written. `drop`
    // is not a secret, but the rule is about the shape of the message rather than this value.
    let rendered = format!("{errors:#?}");
    assert!(
        !rendered.contains("drop"),
        "a refusal must not echo the configured value: {rendered}"
    );
}

/// §12 CC-V-14 — one error per declared control, each naming its own path (§8 V1).
#[test]
fn cc_v_14_every_declared_control_is_named_not_just_the_first() {
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let document = with_security(
        "    unknownSource: drop\n    sanityCheck: true\n    userAgentDenyList: [evil-phone]\n    \
         internalZone: 10.0.0.0/8\n",
    );
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");

    let paths: Vec<String> = errors.iter().map(|e| e.path.to_string()).collect();
    for key in [
        "unknownSource",
        "sanityCheck",
        "userAgentDenyList",
        "internalZone",
    ] {
        let want = format!("cluster.security.{key}");
        assert!(
            paths.contains(&want),
            "declared {key} was not named; got {paths:?}"
        );
    }
    // Ordered by path, so the operator who declared four reads the same four every run.
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "errors must come back ordered by path");
}

/// §12 CC-V-15 — wrong-shaped values are refused all the same. Refusing an unappliable control
/// before typing it is admissible; accepting a value for it is not.
#[test]
fn cc_v_15_a_wrong_shaped_control_cannot_produce_a_config() {
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    // Each of the four gets the wrong shape: a sequence where a scalar was declared, a mapping
    // where a scalar was, a scalar where a sequence was, and a scalar where a mapping was.
    let document = with_security(
        "    unknownSource: [drop, reject]\n    sanityCheck:\n      enabled: true\n    \
         userAgentDenyList: evil-phone\n    internalZone: 42\n",
    );
    let errors = load(document.as_bytes(), &who, &env()).expect_err("must refuse");
    let paths: Vec<String> = errors.iter().map(|e| e.path.to_string()).collect();
    for key in [
        "unknownSource",
        "sanityCheck",
        "userAgentDenyList",
        "internalZone",
    ] {
        let want = format!("cluster.security.{key}");
        assert!(
            paths.contains(&want),
            "a wrong-shaped {key} must still be refused; got {paths:?}"
        );
    }
}

/// An absent or empty `security` block stays valid and carries the fixed Max-Forwards (§8 V6).
#[test]
fn fc6_an_empty_security_block_still_loads_with_the_fixed_max_forwards() {
    let who = identity(1, "a", &[Role::Edge, Role::Registrar]);
    let config = load(good().as_bytes(), &who, &env()).expect("absent security loads");
    assert_eq!(config.security.max_forwards, MAX_FORWARDS);

    let empty = with_security("    {}\n");
    let config = load(empty.as_bytes(), &who, &env()).expect("empty security loads");
    assert_eq!(config.security.max_forwards, MAX_FORWARDS);
}

// ------------------------- DP-16: membership, keys and the shard map (cluster-membership §3–§6) ---

/// The good document with a `keys` section carrying the given already-indented entries.
fn with_keys(entries: &str) -> String {
    good().replace(
        "  locationStore:",
        &format!("  keys:\n{entries}  locationStore:"),
    )
}

/// One well-formed key entry, spelled exactly as `cluster-membership` §4's example spells it.
fn key(id: u8, mint: bool, until: &str) -> String {
    key_over(id, mint, "2026-07-28T12:00:00Z", until)
}

/// The same, with both ends of the verify window stated — for the rules that are about the window
/// rather than about the key.
fn key_over(id: u8, mint: bool, from: &str, until: &str) -> String {
    [
        format!("    - id: {id}"),
        "      algorithm: chacha20-poly1305".to_owned(),
        format!("      secretRef: affinity-key-{id}"),
        format!("      verifyFrom: \"{from}\""),
        format!("      verifyUntil: \"{until}\""),
        format!("      mint: {mint}"),
        String::new(),
    ]
    .join("\n")
}

/// The good document with a `shardMap` section carrying the given already-indented body.
fn with_shard_map(body: &str) -> String {
    good().replace(
        "  locationStore:",
        &format!("  shardMap:\n{body}  locationStore:"),
    )
}

/// The good document with a second member, already indented under `membership`.
fn with_member(entry: &str) -> String {
    good().replace("  locationStore:", &format!("{entry}  locationStore:"))
}

fn who() -> NodeIdentity {
    identity(1, "a", &[Role::Edge, Role::Registrar])
}

fn at_version(document: &str, version: u32) -> String {
    document.replace("version: 42", &format!("version: {version}"))
}

fn paths(errors: &[ConfigError]) -> Vec<String> {
    errors.iter().map(|e| e.path.to_string()).collect()
}

fn error_at<'a>(errors: &'a [ConfigError], path: &str) -> &'a ConfigError {
    errors
        .iter()
        .find(|e| e.path.to_string() == path)
        .unwrap_or_else(|| panic!("no error at {path}: {errors:#?}"))
}

/// **`CC-V-16`.** MB5 — a member on the call path owns flows, so it must say where it is dialled.
#[test]
fn cc_v_16_a_call_path_member_without_an_rpc_endpoint_is_refused() {
    let document = good().replace("      rpc: \"10.0.0.1:7223\"\n", "");
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.membership[0].rpc");
    assert_eq!(error.rule.to_string(), "CC-MB5");
    assert_eq!(error.found, None);
}

/// **`CC-V-17`.** MB5's other direction: an endpoint on a node that owns nothing is a target nobody
/// should reach. `echo` and `e2e-tester` are off the call path (§4 R6).
#[test]
fn cc_v_17_a_member_off_the_call_path_may_not_advertise_an_rpc_endpoint() {
    let document = with_member(
        "    - node: 2\n      name: node-echo\n      zone: a\n      roles: [echo]\n      \
         rpc: \"10.0.0.2:7223\"\n",
    );
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.membership[1].rpc");
    assert_eq!(error.rule.to_string(), "CC-MB5");
    assert!(error.expected.contains("no rpc key"), "{error}");
}

/// **`CC-V-18`.** MB6 — two members advertising one endpoint, named on both sides.
///
/// `affinity-token` §13.1 D5 dials the owner a flow reference names and nothing re-checks that the
/// answer came from it, so an ambiguous endpoint is a request delivered to the wrong node with
/// nothing anywhere disagreeing.
#[test]
fn cc_v_18_two_members_may_not_advertise_one_rpc_endpoint() {
    let document = with_member(
        "    - node: 2\n      name: node-b\n      zone: a\n      roles: [edge]\n      \
         rpc: \"10.0.0.1:7223\"\n",
    );
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.membership[1].rpc");
    assert_eq!(error.rule.to_string(), "CC-MB6");
    assert!(
        error.expected.contains("node-a"),
        "names both holders: {error}"
    );
}

/// **`CC-V-19`.** MB8 — a persisted counter with nowhere to persist is a `boot-second` with extra
/// words, so the reference is required exactly when the mechanism needs one.
#[test]
fn cc_v_19_a_persisted_counter_needs_the_reference_it_is_read_from() {
    let document = good().replace(
        "      rpc: \"10.0.0.1:7223\"\n",
        "      rpc: \"10.0.0.1:7223\"\n      incarnationSource: persisted-counter\n",
    );
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    assert_eq!(
        error_at(&errors, "cluster.membership[0].incarnationRef")
            .rule
            .to_string(),
        "CC-MB8"
    );

    // With the reference, the same document loads and the choice reaches the spec struct.
    let document = document.replace(
        "      incarnationSource: persisted-counter\n",
        "      incarnationSource: persisted-counter\n      incarnationRef: node-a-incarnation\n",
    );
    let config = load(document.as_bytes(), &who(), &env()).expect("loads");
    let member = config.membership.first().expect("one member");
    assert_eq!(
        member.incarnation_source,
        IncarnationSource::PersistedCounter
    );
    assert_eq!(
        member.incarnation_ref.as_deref(),
        Some("node-a-incarnation")
    );
}

/// MB8 — the closed set of mechanisms, spelled in the refusal.
#[test]
fn mb8_an_unknown_incarnation_source_names_the_two_mechanisms() {
    let document = good().replace(
        "      rpc: \"10.0.0.1:7223\"\n",
        "      rpc: \"10.0.0.1:7223\"\n      incarnationSource: whatever\n",
    );
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.membership[0].incarnationSource");
    assert_eq!(error.found.as_deref(), Some("whatever"));
    assert!(error.expected.contains("persisted-counter"), "{error}");
}

/// MB6 — the form is §5 P7's, inherited rather than restated, plus the required port.
///
/// The three values P7 refuses are refused here by the same code path the listener's advertised
/// address goes through: a second spelling of those rules is exactly the defect that spec exists to
/// prevent, so this asserts the *outcome* of sharing one, which is that all three are rejected.
#[test]
fn mb6_an_rpc_endpoint_is_an_advertised_address_with_a_port() {
    for (declared, why) in [
        ("0.0.0.0:7223", "unspecified"),
        ("10.0.0.1", "no port"),
        ("10.0.0.1:0", "port zero"),
        ("", "empty"),
    ] {
        let document = good().replace("10.0.0.1:7223", declared);
        let errors = load(document.as_bytes(), &who(), &env())
            .err()
            .unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|e| e.path.to_string() == "cluster.membership[0].rpc"),
            "an rpc of `{declared}` is {why} and must be refused: {errors:#?}"
        );
    }
}

/// MB4 — a member `name` is unique in the document, byte-compared (§6 I4).
///
/// `shardMap[].owner` resolves by name (SM2), so two members answering to one name would make an
/// ownership assignment ambiguous — DS2's "a shard accepting at two nodes" through the front door.
#[test]
fn mb4_two_members_may_not_share_a_name() {
    let document = with_member(
        "    - node: 2\n      name: node-a\n      zone: a\n      roles: [edge]\n      \
         rpc: \"10.0.0.2:7223\"\n",
    );
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.membership[1].name");
    assert_eq!(error.rule.to_string(), "CC-MB4");
    assert!(
        error.expected.contains("node 1"),
        "names the other holder: {error}"
    );
}

/// **`CC-I-1`.** Two members with one id, refused naming both holders.
///
/// `affinity-token` §12.2 CT1 makes this a correctness input rather than a convention: two nodes
/// sharing an id give two different connections one flow identity, and it is the same id
/// `media-relay` §6.2 C2 needs cluster-unique for NG cookies.
#[test]
fn cc_i_1_two_members_with_one_id_are_refused_naming_both() {
    let document = with_member(
        "    - node: 1\n      name: node-b\n      zone: a\n      roles: [edge]\n      \
         rpc: \"10.0.0.2:7223\"\n",
    );
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.membership[1].node");
    assert_eq!(error.path.to_string(), "cluster.membership[1].node");
    assert_eq!(error.rule.to_string(), "CC-I2");
    assert!(
        error.expected.contains("node-a"),
        "names both holders: {error}"
    );
}

// --------------------------------------------------------------- keys (cluster-membership §4) ---

/// A key set the loader accepts, so each test below can break exactly one rule of §4.
#[test]
fn ky1_the_six_attributes_load_into_the_key_the_token_library_consumes() {
    let document = with_keys(&key(3, true, "2026-08-04T12:00:30Z"));
    let config = load(document.as_bytes(), &who(), &env()).expect("loads");
    let entry = config.keys.first().expect("one key");
    assert_eq!(entry.id, 3);
    assert_eq!(entry.algorithm, KeyAlgorithm::ChaCha20Poly1305);
    assert_eq!(entry.secret_ref, "affinity-key-3");
    assert!(entry.mint);
    // KY4's instants, resolved to the whole seconds `affinity-token` §8 S2 compares `now` against.
    assert_eq!(entry.verify_from, 1_785_240_000);
    assert_eq!(entry.verify_until, 1_785_844_830);
}

/// **`CC-V-10`.** KY3 — an inline `secret` is refused citing V9, and the refusal does not echo it.
///
/// Two different problems lead an operator to two different actions: V2's "unrecognised key" would
/// read as "this schema has no notion of a secret", which is the opposite of true. And the message
/// **describes** what was written rather than quoting it — this rule fires exactly when a real
/// secret is sitting in the field, so a refusal that printed it would be the defect enforcing itself.
#[test]
fn cc_v_10_an_inline_key_secret_is_refused_by_reference_not_as_an_unknown_key() {
    let secret = "1f8b0900000000000003ababababababababababababababab";
    let document = with_keys(&key(3, true, "2026-08-04T12:00:30Z").replace(
        "      secretRef: affinity-key-3\n",
        &format!("      secretRef: affinity-key-3\n      secret: \"{secret}\"\n"),
    ));
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.keys[0].secret");
    assert_eq!(error.path.to_string(), "cluster.keys[0].secret");
    assert_eq!(error.rule.to_string(), "CC-V9");
    assert_eq!(error.found.as_deref(), Some("an inline key secret"));

    let rendered = format!("{errors:#?}");
    assert!(
        !rendered.contains(secret),
        "a refusal must not echo the secret it refuses"
    );
}

/// **`CC-K-5`.** Two entries sharing an id while both windows are open (`affinity-token` §6).
///
/// Ids may wrap over the years; two open windows on one id make key selection ambiguous for exactly
/// the tokens rotation exists to keep verifying.
#[test]
fn cc_k_5_two_keys_may_not_share_an_id_with_overlapping_windows() {
    let document = with_keys(&format!(
        "{}{}",
        key(3, true, "2026-08-04T12:00:30Z"),
        key(3, false, "2026-08-11T12:00:30Z")
    ));
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.keys[1].id");
    assert_eq!(error.path.to_string(), "cluster.keys[1].id");
    assert_eq!(error.rule.to_string(), "CC-KY1");
    assert!(error.expected.contains("affinity-token §6"), "{error}");
}

/// **`CC-K-6`.** KY5 — exactly one entry mints, at any configuration version.
#[test]
fn cc_k_6_two_minting_keys_are_refused() {
    let document = with_keys(&format!(
        "{}{}",
        key(3, true, "2026-08-04T12:00:30Z"),
        key(4, true, "2026-08-11T12:00:30Z")
    ));
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.keys");
    assert_eq!(error.rule.to_string(), "CC-KY5");
}

/// **`CC-V-20`.** KY5's other half — a declared section in which nothing mints is refused.
///
/// A cluster that mints nothing Record-Routes nothing, and would fail on its first dialog-forming
/// request rather than at load.
#[test]
fn cc_v_20_a_key_section_with_no_minting_key_is_refused() {
    let document = with_keys(&key(3, false, "2026-08-04T12:00:30Z"));
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.keys");
    assert_eq!(error.rule.to_string(), "CC-KY5");
    assert_eq!(error.found.as_deref(), Some("no key marked mint: true"));
}

/// KY4 — the window is absolute, non-empty, and spelled in UTC.
#[test]
fn ky4_a_validity_window_is_two_absolute_instants_in_order() {
    // Backwards: an entry that can never verify anything.
    let document = with_keys(&key(3, true, "2026-07-01T12:00:00Z"));
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    assert_eq!(
        error_at(&errors, "cluster.keys[0].verifyUntil")
            .rule
            .to_string(),
        "CC-KY4"
    );

    // Relative is unrepresentable on purpose: the loader has no clock, so `7d` would resolve
    // against whatever moment each node happened to reload.
    let document = with_keys(&key(3, true, "7d"));
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    assert_eq!(
        error_at(&errors, "cluster.keys[0].verifyUntil")
            .rule
            .to_string(),
        "CC-KY4"
    );

    // An offset spelling names the same instant and is a second way to write it.
    let document = with_keys(&key(3, true, "2026-08-04T14:00:30+02:00"));
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    assert!(
        error_at(&errors, "cluster.keys[0].verifyUntil")
            .expected
            .contains('Z'),
        "the refusal names the spelling it wants"
    );
}

/// The instant reader, against the two boundaries a hand-written date parser gets wrong.
#[test]
fn ky4_the_instant_reader_agrees_with_the_calendar() {
    assert_eq!(parse_rfc3339_utc("1970-01-01T00:00:00Z"), Some(0));
    // A leap day exists in 2024 and does not in 2026.
    assert_eq!(
        parse_rfc3339_utc("2024-02-29T00:00:00Z"),
        Some(1_709_164_800)
    );
    assert_eq!(parse_rfc3339_utc("2026-02-29T00:00:00Z"), None);
    // 1900 is not a leap year and 2000 is — the rule a modulo-4 parser gets wrong.
    assert_eq!(parse_rfc3339_utc("1900-02-29T00:00:00Z"), None);
    assert_eq!(parse_rfc3339_utc("2000-02-29T00:00:00Z"), Some(951_782_400));
    assert_eq!(parse_rfc3339_utc("2026-13-01T00:00:00Z"), None);
    assert_eq!(parse_rfc3339_utc("2026-07-28T24:00:00Z"), None);
    // The fractional part is parsed and discarded: §8 S2 compares whole seconds.
    assert_eq!(
        parse_rfc3339_utc("2026-07-28T12:00:00.250Z"),
        parse_rfc3339_utc("2026-07-28T12:00:00Z")
    );
}

/// KY6 — §8 V8's ceiling of sixteen entries, adopted unchanged.
#[test]
fn ky6_the_key_ceiling_is_checked_at_load() {
    let entries: String = (1..=17)
        .map(|id| key(id, id == 1, "2026-08-04T12:00:30Z"))
        .collect();
    let document = with_keys(&entries);
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.keys");
    assert_eq!(error.rule.to_string(), "CC-V8");
    assert_eq!(error.found.as_deref(), Some("17"));
}

/// §8 V2 still closes one level down: KY1's six attributes are the whole interface.
#[test]
fn ky1_a_seventh_attribute_on_a_key_is_refused() {
    let document = with_keys(&key(3, true, "2026-08-04T12:00:30Z").replace(
        "      mint: true\n",
        "      mint: true\n      rotateEvery: 7d\n",
    ));
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    assert_eq!(
        error_at(&errors, "cluster.keys[0].rotateEvery")
            .rule
            .to_string(),
        "CC-V2"
    );
}

// ---------------------------------------------------------- the shard map (cluster-membership §5) ---

/// SM1/SM2 — a total map whose owners are declared members loads, and reaches the spec struct.
#[test]
fn sm1_a_total_shard_map_loads_with_ds4s_default_drain_timeout() {
    let document = with_shard_map("    shards:\n      - id: 1\n        owner: node-a\n");
    let config = load(document.as_bytes(), &who(), &env()).expect("loads");
    let map = config.shard_map.as_ref().expect("a shard map");
    assert_eq!(map.drain_timeout_ms, 30_000, "§9.4 DS4's declared default");
    assert_eq!(map.shards.len(), 1);
    assert_eq!(map.shards.first().expect("one shard").owner, "node-a");
}

/// **`CC-V-4`.** SM2 — an owner absent from `membership` is a §8 V5 cross-section failure.
#[test]
fn cc_v_4_a_shard_owner_absent_from_membership_is_refused() {
    let document = with_shard_map("    shards:\n      - id: 1\n        owner: node-z\n");
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.shardMap.shards[0].owner");
    assert_eq!(error.rule.to_string(), "CC-V5");
    assert_eq!(error.found.as_deref(), Some("node-z"));
}

/// **`CC-V-21`.** SM1 — the list is the shard space and it is total: a gap is refused naming it.
///
/// A shard with no owner is a slice of the registration key space for which no REGISTER can be
/// accepted, and it would surface as a tenant's phones going quiet rather than as a config error.
#[test]
fn cc_v_21_a_shard_map_with_a_gap_is_refused() {
    let document = with_shard_map(
        "    shards:\n      - id: 1\n        owner: node-a\n      - id: 3\n        owner: node-a\n",
    );
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.shardMap.shards");
    assert_eq!(error.rule.to_string(), "CC-SM1");
    assert!(
        error.found.as_deref().unwrap_or_default().contains('2'),
        "{error}"
    );
}

/// **`CC-V-22`.** SM3 — a shard owns registration state, so its owner runs a registrar.
#[test]
fn cc_v_22_a_shard_owner_that_runs_no_registrar_is_refused() {
    let document = with_member(
        "    - node: 2\n      name: node-b\n      zone: a\n      roles: [edge]\n      \
         rpc: \"10.0.0.2:7223\"\n",
    )
    .replace(
        "  locationStore:",
        "  shardMap:\n    shards:\n      - id: 1\n        owner: node-b\n  locationStore:",
    );
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.shardMap.shards[0].owner");
    assert_eq!(error.rule.to_string(), "CC-SM3");
}

/// **`CC-S-7`.** DS4's range, checked at load.
///
/// Below the floor a drain would expire while an ordinary contended write was still legitimately
/// retrying (`location-service` §5.1 S10).
#[test]
fn cc_s_7_a_drain_timeout_below_the_range_is_refused() {
    let document =
        with_shard_map("    drainTimeout: 2s\n    shards:\n      - id: 1\n        owner: node-a\n");
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.shardMap.drainTimeout");
    assert_eq!(error.rule.to_string(), "CC-DS4");
    assert_eq!(error.found.as_deref(), Some("2s"));
}

/// A duration carries its unit. Every other duration in this schema is milliseconds, so a bare `30`
/// would be two plausible values a thousand-fold apart and the loader guesses at neither.
#[test]
fn ds4_a_drain_timeout_without_its_unit_is_refused() {
    let document =
        with_shard_map("    drainTimeout: 30\n    shards:\n      - id: 1\n        owner: node-a\n");
    let errors = load(document.as_bytes(), &who(), &env()).expect_err("must refuse");
    assert!(
        error_at(&errors, "cluster.shardMap.drainTimeout")
            .expected
            .contains("30s"),
        "the refusal names the form it wants"
    );
}

/// Neither section reaches a consumer in this build, and both say so (`FC-2`).
///
/// Validating a value is not applying it — the mistake `FC-4` found when `domains` sat in a struct
/// field unread for a release. `AF-4`'s mint/verify library reaches no driver field yet and the
/// shard handoff is `RG-5`'s, so a document that declares either gets exactly what this warns about.
#[test]
fn fc2_keys_and_the_shard_map_are_validated_and_reported_as_unapplied() {
    let document = with_keys(&key(3, true, "2026-08-04T12:00:30Z")).replace(
        "  locationStore:",
        "  shardMap:\n    shards:\n      - id: 1\n        owner: node-a\n  locationStore:",
    );
    let config = load(document.as_bytes(), &who(), &env()).expect("loads");
    let reported: Vec<String> = config.unapplied.iter().map(ToString::to_string).collect();
    for expected in ["cluster.keys", "cluster.shardMap"] {
        assert!(reported.iter().any(|path| path == expected), "{reported:?}");
    }
    // And they are genuinely read, which is what makes the report a report rather than a shrug.
    assert_eq!(config.keys.len(), 1);
    assert!(config.shard_map.is_some());
}

/// **`CC-V-23`.** A year RFC 3339 §5.6 cannot express is refused, not reduced to an instant.
///
/// `load` documents itself pure and total in its inputs, §8 V10 makes refusing the document the only
/// failure mode there is, and `AGENTS.md` rule 3 forbids panicking on input. This spelling reached
/// `days_from_civil(year, …) * 86_400` with `year` parsed unbounded, so it panicked under
/// `debug-assertions` and — the worse half — **wrapped** under `-O`, accepting a verify window
/// nobody wrote. A rotation rule judged against a wrapped instant is a safety rule switched off
/// silently, and silence is the property `cluster-membership` §7.1 RB9 exists to deny the document.
#[test]
fn cc_v_23_a_year_outside_rfc_3339_is_refused_rather_than_wrapped() {
    for year in ["300000000000", "9223372036854775807", "99999", "10000"] {
        let document = with_keys(&key_over(
            3,
            true,
            &format!("{year}-01-01T00:00:00Z"),
            "2026-09-01T12:00:00Z",
        ));
        // Reported rather than asserted away, because the two builds fail differently and the
        // release one is the dangerous half: debug panicked, `-O` accepted a wrapped instant.
        let errors = match load(document.as_bytes(), &who(), &env()) {
            Ok(config) => panic!(
                "year `{year}` is not `4DIGIT` and must be refused; loaded verifyFrom = {:?}",
                config.keys.first().map(|entry| entry.verify_from)
            ),
            Err(errors) => errors,
        };
        let error = error_at(&errors, "cluster.keys[0].verifyFrom");
        assert_eq!(error.rule.to_string(), "CC-KY4", "year `{year}`");
    }
}

/// **`CC-V-24`.** RFC 3339 §5.6's grammar is the whole grammar: `4DIGIT`, `2DIGIT`, `"." 1*DIGIT`.
///
/// `str::parse::<i64>` accepts a leading sign and any number of digits, which is how every spelling
/// below became an instant this loader accepted. The first one is why the rule is not cosmetic: it
/// parses to an instant **twenty-six hours away** from the one written, so a document could state a
/// verify window and get a different one without a single error.
#[test]
fn cc_v_24_rfc_3339_section_5_6_is_the_whole_grammar() {
    for spelling in [
        "2026-07-28T-1:-5:-9Z",
        "+2026-07-28T12:00:00Z",
        "2026-7-8T1:2:3Z",
        "2026-07-28T+1:00:00Z",
        "2026-07-28T12:00:00.abcZ",
        "2026-07-28T12:00:00.Z",
        "2026-07-28T12:00:00.1.2Z",
    ] {
        let document = with_keys(&key(3, true, spelling));
        let errors = load(document.as_bytes(), &who(), &env())
            .err()
            .unwrap_or_else(|| panic!("`{spelling}` is not RFC 3339 §5.6 and must be refused"));
        let error = error_at(&errors, "cluster.keys[0].verifyUntil");
        assert_eq!(error.rule.to_string(), "CC-KY4", "`{spelling}`");
        // The *reason* matters as much as the refusal, and this assertion is why. Every spelling
        // above parsed to some instant before `verifyFrom` under the old grammar, so all seven were
        // already refused — as "a window that never opens", a KY4 error at this very path. A test
        // that stopped at the rule id would have passed against the defect it was written for
        // (`CF-12`). KY4's parse failure echoes the text it could not read; the window error names
        // the window.
        assert_eq!(
            error.found.as_deref(),
            Some(spelling),
            "refused as a window rather than as a spelling: {error}"
        );
    }
}

/// Every spelling §5.6 *does* admit still loads, so the rule above narrowed the grammar to the
/// grammar and not to a habit — lower-case `t`/`z` are RFC 3339's own alternates, and a fraction is
/// parsed and discarded because `affinity-token` §8 S2 compares whole seconds.
#[test]
fn ky4_the_spellings_rfc_3339_admits_are_still_accepted() {
    for spelling in [
        "2026-09-01T12:00:00Z",
        "2026-09-01t12:00:00z",
        "2026-09-01T12:00:00.5Z",
        "2026-09-01T23:59:60Z",
        "2028-02-29T00:00:00Z",
        "9999-12-31T23:59:59Z",
    ] {
        let document = with_keys(&key(3, true, spelling));
        load(document.as_bytes(), &who(), &env())
            .unwrap_or_else(|e| panic!("`{spelling}` is RFC 3339 §5.6 and must load: {e:?}"));
    }
}

/// **`CC-K-7`.** RL11's second half — the *incoming* mint key's window must cover the same bound.
///
/// Without it the first half is satisfiable by a document that retires nothing and still strands
/// every record: a key whose window is a minute wide mints tokens that stop verifying long inside
/// `W`, and the next rotation inherits the problem rather than causing it. Judged as a width because
/// §2 D1 forbids the loader a clock — RB2's `verifyUntil ≥ t_activate + W` is a wall-clock statement
/// whose clock-free consequence is that the window is at least `W` wide.
#[test]
fn cc_k_7_an_incoming_mint_key_must_cover_the_overlap_window() {
    let narrow = |mints: bool| {
        with_keys(&format!(
            "{}{}",
            key(3, !mints, "2026-09-01T12:00:00Z"),
            key_over(4, mints, "2026-07-28T12:00:00Z", "2026-07-28T12:01:00Z")
        ))
    };
    // The narrow window is accepted at `load`: §9.1 RL3 makes every transition rule vacuous where
    // there is no predecessor, and this rule is a transition rule.
    let active = load(narrow(false).as_bytes(), &who(), &env()).expect("the active document loads");

    let errors = reload(
        &active,
        at_version(&narrow(true), 43).as_bytes(),
        &who(),
        &env(),
    )
    .expect_err("must refuse");
    let error = error_at(&errors, "cluster.keys[1].verifyUntil");
    assert_eq!(error.rule.to_string(), "CC-RL11");
    assert!(
        error.expected.contains("86430"),
        "the refusal names the bound it computed: {error}"
    );
}

/// `W` is `max(L, E_max) + S` computed from the document, not a constant — `cluster-membership` §7.1
/// RB1: one tenant raising its registration ceiling lengthens the rotation for the whole cluster.
#[test]
fn rl11_the_overlap_window_follows_the_documents_largest_tenant_expiry() {
    // A window of two days covers the default `W` of 86 430 s and not a `W` raised past it.
    let two_days = |mints: bool| {
        with_keys(&format!(
            "{}{}",
            key(3, !mints, "2026-09-01T12:00:00Z"),
            key_over(4, mints, "2026-07-28T12:00:00Z", "2026-07-30T12:00:00Z")
        ))
    };
    let active = load(two_days(false).as_bytes(), &who(), &env()).expect("loads");
    reload(
        &active,
        at_version(&two_days(true), 43).as_bytes(),
        &who(),
        &env(),
    )
    .expect("two days covers the default overlap window");

    // `E_max` is the tenant's `expiry.max`, and RB1 makes it the document's largest.
    let raised = |document: &str| {
        document.replace(
            "      id: 1\n",
            "      id: 1\n      expiry:\n        max: 604800\n",
        )
    };
    let active = load(raised(&two_days(false)).as_bytes(), &who(), &env()).expect("loads");
    let errors = reload(
        &active,
        raised(&at_version(&two_days(true), 43)).as_bytes(),
        &who(),
        &env(),
    )
    .expect_err("a week-long E_max moves the bound past a two-day window");
    assert_eq!(
        error_at(&errors, "cluster.keys[1].verifyUntil")
            .rule
            .to_string(),
        "CC-RL11"
    );
}

// ---------------------------------------------------------------------- reload (§9, §6 RD1) ---

/// §7's registry gives every recognised section a reload class, and this proves the table did not
/// stop at the sections that happened to have one when it was written.
///
/// RL1 is why it matters: the class is a property of the field, and a section the node classified by
/// falling through a default is a section the operator and the node can disagree about.
#[test]
fn rl1_every_recognised_section_declares_a_reload_class() {
    for section in CLUSTER_KEYS.iter().chain(DEFERRED_SECTIONS) {
        assert!(
            RELOAD_CLASSES.iter().any(|(name, _)| name == section),
            "`{section}` is recognised by the closed world and has no §7 reload class"
        );
    }
}

/// **`CC-K-1`.** Adding a verify-only key is accepted, and the plan says which section moved.
///
/// This is `affinity-token` §6 K1's first step: `B` is distributed with `mint: false` and an
/// already-open window, so every node can verify a `B` record before any node mints one. Nothing
/// else in the document changes, so nothing else in the node does — which is RD1 and RL12, and is
/// what "reloadable without restart" means for keys.
#[test]
fn cc_k_1_adding_a_verify_only_key_is_a_reload_and_disturbs_nothing() {
    let active = load(
        with_keys(&key(3, true, "2026-09-01T12:00:00Z")).as_bytes(),
        &who(),
        &env(),
    )
    .expect("the active document loads");
    let next = at_version(
        &with_keys(&format!(
            "{}{}",
            key(3, true, "2026-09-01T12:00:00Z"),
            key(4, false, "2026-09-08T12:00:00Z")
        )),
        43,
    );

    let (config, plan) = reload(&active, next.as_bytes(), &who(), &env()).expect("accepted");
    assert_eq!(
        plan.changed
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["cluster.keys".to_owned()],
        "the plan reports keys changed, and reports nothing else moving"
    );
    assert_eq!(config.keys.len(), 2);
}

/// **`CC-K-2`.** Flipping `mint` to a key the active document already carried is K3, and accepted.
#[test]
fn cc_k_2_flipping_mint_to_an_already_distributed_key_is_accepted() {
    let both = |first_mints: bool| {
        with_keys(&format!(
            "{}{}",
            key(3, first_mints, "2026-09-01T12:00:00Z"),
            key(4, !first_mints, "2026-09-08T12:00:00Z")
        ))
    };
    let active = load(both(true).as_bytes(), &who(), &env()).expect("the active document loads");
    let next = at_version(&both(false), 43);

    let (config, plan) = reload(&active, next.as_bytes(), &who(), &env()).expect("accepted");
    assert_eq!(plan.changed.len(), 1);
    assert!(config.keys.iter().any(|entry| entry.id == 4 && entry.mint));
}

/// **`CC-K-3`.** RL10 — a mint flipped to a key the active version never carried is refused, naming
/// the key id and both versions.
///
/// K1 and K3 collapsed into one step: it produces records some healthy node cannot verify yet, and
/// the node that minted them reports itself healthy because it verifies its own.
#[test]
fn cc_k_3_a_mint_flip_to_an_undistributed_key_is_refused() {
    let active = load(
        with_keys(&key(3, true, "2026-09-01T12:00:00Z")).as_bytes(),
        &who(),
        &env(),
    )
    .expect("the active document loads");
    let next = at_version(
        &with_keys(&format!(
            "{}{}",
            key(3, false, "2026-09-01T12:00:00Z"),
            key(5, true, "2026-09-08T12:00:00Z")
        )),
        43,
    );

    let errors = reload(&active, next.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.keys[1].mint");
    assert_eq!(error.rule.to_string(), "CC-RL10");
    let found = error.found.clone().unwrap_or_default();
    assert!(
        found.contains("key id 5") && found.contains("43") && found.contains("42"),
        "{error}"
    );
}

/// **`CC-K-4`.** RL11 — the outgoing mint key's verify window is not brought forward.
///
/// Judged from the *declared* windows, because §2 D1 forbids the loader a clock: whether
/// `max(L, E_max) + S` has actually elapsed is `cluster-membership` §7.1 RB5's, addressed to an
/// operator with a wall clock.
#[test]
fn cc_k_4_bringing_the_mint_keys_verify_window_forward_is_refused() {
    let active = load(
        with_keys(&key(3, true, "2026-09-01T12:00:00Z")).as_bytes(),
        &who(),
        &env(),
    )
    .expect("the active document loads");
    let next = at_version(&with_keys(&key(3, true, "2026-08-01T12:00:00Z")), 43);

    let errors = reload(&active, next.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.keys[0].verifyUntil");
    assert_eq!(error.rule.to_string(), "CC-RL11");
    assert!(error.expected.contains("max(L, E_max) + S"), "{error}");
}

/// RL11's other half: removing the key that is still minting is refused, not merely discouraged.
///
/// RB9 makes emergency retirement a rolling restart on purpose — a safety rule that could be
/// switched off from the document is one that gets switched off during an incident.
#[test]
fn rl11_removing_the_outgoing_mint_key_is_refused() {
    let active = load(
        with_keys(&format!(
            "{}{}",
            key(3, true, "2026-09-01T12:00:00Z"),
            key(4, false, "2026-09-08T12:00:00Z")
        ))
        .as_bytes(),
        &who(),
        &env(),
    )
    .expect("the active document loads");
    let next = at_version(&with_keys(&key(4, true, "2026-09-08T12:00:00Z")), 43);

    let errors = reload(&active, next.as_bytes(), &who(), &env()).expect_err("must refuse");
    assert_eq!(
        error_at(&errors, "cluster.keys").rule.to_string(),
        "CC-RL11"
    );
}

/// **`CC-D-7`.** D10 — a reload at or below the active version is rejected and nothing changes.
#[test]
fn cc_d_7_a_reload_that_does_not_advance_the_version_is_rejected() {
    let active = load(good().as_bytes(), &who(), &env()).expect("the active document loads");
    let unchanged = active.clone();
    let next = good().replace("  environment: dev", "  environment: dev\n  nat: {}");

    let errors = reload(&active, next.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "version");
    assert_eq!(error.rule.to_string(), "CC-D10");
    assert_eq!(active, unchanged, "the active configuration is untouched");
}

/// **`CC-D-8`.** RL2 — a document changing a `rollout`-class field is rejected **as a reload**,
/// naming the field, and the reloadable change riding along with it is not applied either.
///
/// Atomicity is the whole content of the rule: the document is validated and then either applied or
/// not, so an operator never has to reason about which half of a push landed.
#[test]
fn cc_d_8_a_rollout_class_change_is_rejected_as_a_reload() {
    let document = |bind: &str, trunk: &str| {
        format!(
            r"
apiVersion: sipx.dev/v1alpha1
version: 42
cluster:
  name: acme
  environment: dev
  zones: [a]
  listener:
    - roles: [edge, registrar]
      transport: udp
      bind: 0.0.0.0:5060
      advertise: 203.0.113.10:5060
    - roles: [e2e-tester]
      transport: udp
      bind: {bind}
      advertise: 203.0.113.10:5062
  membership:
    - node: 1
      name: node-a
      zone: a
      roles: [edge, registrar]
      rpc: '10.0.0.1:7223'
  trunk:
    - name: {trunk}
  locationStore:
    backend: memory
  tenant:
    - name: default
      id: 1
      domains: [acme.example]
"
        )
    };
    let active = load(
        document("0.0.0.0:5062", "carrier-a").as_bytes(),
        &who(),
        &env(),
    )
    .expect("the active document loads");
    let next = at_version(&document("0.0.0.0:5063", "carrier-b"), 43);

    let errors = reload(&active, next.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.listener[1].bind");
    assert_eq!(error.path.to_string(), "cluster.listener[1].bind");
    assert_eq!(error.rule.to_string(), "CC-RL2");
    // The trunk change rode along and is not applied: a rejected reload applies nothing at all.
    assert!(
        !paths(&errors).iter().any(|path| path == "cluster.trunk"),
        "the trunk change is reloadable and is not what was refused: {errors:#?}"
    );
}

/// **`CC-I-4`.** I3 and RD4 — a `node` id is not re-pointed to a different `name` by a reload.
///
/// Reusing an id early is indistinguishable, on the wire, from the record it collides with. The
/// loader owns the version-to-version half; §7.2 RB11's `W` wait is the operator's, because it needs
/// a wall clock this loader does not have.
#[test]
fn cc_i_4_a_node_id_is_not_re_pointed_to_a_different_name() {
    let active = load(good().as_bytes(), &who(), &env()).expect("the active document loads");
    let next = at_version(&good().replace("name: node-a", "name: node-b"), 43);

    let errors = reload(&active, next.as_bytes(), &who(), &env()).expect_err("must refuse");
    let error = error_at(&errors, "cluster.membership[0].node");
    assert_eq!(error.rule.to_string(), "CC-I3");
    let found = error.found.clone().unwrap_or_default();
    assert!(
        found.contains("node-a") && found.contains("node-b"),
        "{error}"
    );
}

/// RD2 — adding or removing a member is a reload, with no restart and no quiescence.
///
/// And RD1 with it: the plan names `membership` alone, so nothing that rebinds a listener, closes a
/// connection or expires a registration is part of what the node is being asked to do.
#[test]
fn rd2_adding_a_member_is_a_reload() {
    let active = load(good().as_bytes(), &who(), &env()).expect("the active document loads");
    let next = at_version(
        &with_member(
            "    - node: 2\n      name: node-b\n      zone: a\n      roles: [edge]\n      \
             rpc: \"10.0.0.2:7223\"\n",
        ),
        43,
    );

    let (config, plan) = reload(&active, next.as_bytes(), &who(), &env()).expect("accepted");
    assert_eq!(
        plan.changed
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["cluster.membership".to_owned()]
    );
    assert_eq!(config.membership.len(), 2);
}

/// A document that only reordered a section's keys is not a change.
///
/// Without this the canonical rendering would be a spelling test rather than a content test, and an
/// operator who sorted their YAML would be told to restart the fleet.
#[test]
fn rl2_reordering_a_rollout_sections_keys_is_not_a_change() {
    let active = load(good().as_bytes(), &who(), &env()).expect("the active document loads");
    let reordered = good().replace(
        "      transport: udp\n      bind: 0.0.0.0:5060\n",
        "      bind: 0.0.0.0:5060\n      transport: udp\n",
    );
    let next = at_version(&reordered, 43);

    let (_, plan) = reload(&active, next.as_bytes(), &who(), &env()).expect("accepted");
    assert!(plan.changed.is_empty(), "{plan:?}");
}

/// §6 RD3 — a reload that changes *this* node's own `zone` or `roles` is refused, and it is refused
/// by §5 P3's cross-check rather than by a second rule beside it.
///
/// Identity is a start-up input (P1) and no document can change what a process was started as.
#[test]
fn rd3_a_reload_that_changes_this_nodes_own_identity_is_refused() {
    let active = load(good().as_bytes(), &who(), &env()).expect("the active document loads");
    let next = at_version(
        &good().replace("      zone: a\n      roles:", "      zone: b\n      roles:"),
        43,
    );

    let errors = reload(&active, next.as_bytes(), &who(), &env()).expect_err("must refuse");
    assert!(rules(&errors).contains(&"CC-P3".to_owned()), "{errors:#?}");
}

/// **`CC-V-8`.** `security.maxForwards` absent loads with 70 (§8 V6).
///
/// The literal is RFC 3261 §16.6 step 3's own number rather than the constant that holds it: a test
/// comparing `MAX_FORWARDS` to `MAX_FORWARDS` would keep passing if the constant moved, and the
/// whole content of V6 is that this value is fixed somewhere other than in this schema.
#[test]
fn cc_v_8_an_absent_max_forwards_loads_with_the_rfcs_value() {
    let config = load(good().as_bytes(), &who(), &env()).expect("loads");
    assert_eq!(config.security.max_forwards, 70);
}

/// **`CC-V-11`.** `tenant[].expiry` omitted keeps location-service §5.2's own defaults (§8 V3).
///
/// The three literals are that spec's, adopted unchanged rather than restated differently — a
/// number spelled twice is a number that drifts, which is what `FC-4` found when the document's
/// quota loaded and the library's default was what ran.
#[test]
fn cc_v_11_an_omitted_expiry_block_keeps_the_owning_specs_defaults() {
    let config = load(good().as_bytes(), &who(), &env()).expect("loads");
    let policy = &config.tenants.first().expect("one tenant").policy;
    assert_eq!(policy.default_expires, 3_600);
    assert_eq!(policy.min_expires, 60);
    assert_eq!(policy.max_expires, 86_400);
}
