#!/usr/bin/env python3
"""Turn a rendered SipxCluster into the document a node reads.

`helm template` emits a SipxCluster resource whose `spec` is two things at once: a few keys the
operator needs in order to *deploy* the cluster, and — emitted verbatim by `templates/sipxcluster.yaml`
— the cluster configuration document of `docs/specs/cluster-config.md`. A node reads the second half
wrapped in the two version fields of §3:

    apiVersion: sipx.dev/v1alpha1     # which schema this document is written against
    version: 1                        # which configuration this document *is*
    cluster: { … }                     # §7's sections

So this script subtracts the operator's keys and rebuilds that wrapper, which is what makes the
chart's `cluster:` tree checkable against the loader instead of by eye. It is a pure transform:
YAML in, YAML out, no network and no cluster.

`version` is not in the chart (§3 D9 — the operator sets it from `metadata.generation`), so it is
supplied here; nothing downstream of `load` depends on which value, only that it is a u32.
"""

import argparse
import sys

import yaml

# The keys `templates/sipxcluster.yaml` writes into `spec` *itself*, rather than copying out of
# `.Values.cluster`. Everything else in `spec` is configuration.
#
# The failure direction is deliberate: a key the template starts writing and nobody adds here is
# treated as configuration, and the loader's closed world (§8 V2) then refuses it by name. That is a
# red check somebody fixes, rather than a config key silently leaking into the operator's half.
OPERATOR_KEYS = ("image", "roles", "nodeSelector", "tolerations")


def node_document(rendered: str, version: int) -> dict:
    resources = [doc for doc in yaml.safe_load_all(rendered) if doc]
    clusters = [doc for doc in resources if doc.get("kind") == "SipxCluster"]
    if len(clusters) != 1:
        raise SystemExit(
            f"expected exactly one SipxCluster in the rendered chart, found {len(clusters)}"
        )
    spec = clusters[0].get("spec")
    if not isinstance(spec, dict):
        raise SystemExit("the SipxCluster has no spec mapping")

    cluster = {key: value for key, value in spec.items() if key not in OPERATOR_KEYS}
    if not cluster:
        raise SystemExit("the SipxCluster spec carries no configuration keys at all")

    return {
        # The resource's API version *is* the document's schema version — one version field on the
        # thing being versioned.
        "apiVersion": clusters[0]["apiVersion"],
        "version": version,
        "cluster": cluster,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--version",
        type=int,
        default=1,
        help="the configuration version to stamp on the document (default: 1)",
    )
    args = parser.parse_args()

    document = node_document(sys.stdin.read(), args.version)
    yaml.safe_dump(document, sys.stdout, sort_keys=False, default_flow_style=False)


if __name__ == "__main__":
    main()
