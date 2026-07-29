# Design: The public documentation site

**Status:** active · **Pillar:** Foundation · **Epic:** `docs-site` ·
**Stories:** DX-1 … DX-9

## Why

The site a stranger lands on should say what this does and how to run it, not what we plan next.

Until `DX-1`, the published site was a verbatim view of `docs/`: the docs plugin read `../docs`
and excluded only `stories/**`, `archive/**` and `README.md`. Everything else went out —
the roadmap with its milestone tables and "as of" dates, thirteen design records of which nine
still said `Status: proposed`, ten normative specs, and a generated conformance report. The
landing page still claimed "nothing forwards a SIP message yet", which M1 disproved three
releases earlier.

There was no install page, no quickstart, no configuration guide, and no CLI reference. A reader
who wanted to *use* this had nothing, while a reader who wanted to *contribute* got material that
is more useful with the repository open anyway.

Publishing internal planning material also has a cost beyond confusion: it dates fast. A design
record that says `proposed` is honest inside the repo and misleading on a product site, because a
visitor cannot tell a decision record from a promise.

## Approach

**Two trees, one published**, matching the arrangement the sipx kernel already uses. `docs/`
becomes internal and unpublished. `website/docs/` becomes a hand-authored end-user site with its
own voice, its own structure, and no generated content. Site pages reach specs, designs and the
roadmap by absolute GitHub URL.

The information architecture is a ladder, and the order is the argument:

1. **What sipx-clstr is** — including an honest capability matrix.
2. **Getting started** — a node and a forwarded call, with nothing but Rust and Python.
3. **Guides** — everything that runs today.
4. **Clustering (preview)** — specified, not shipped.
5. **Operate (preview)** — deployment, scaling, HA.
6. **Migrate** — the concept maps for people arriving from an existing deployment.
7. **Reference** — CLI, configuration, conformance.

Status is carried in the sidebar label, so a reader knows a section is unshipped before they open
it, and restated in the page's own words once they do. The vocabulary is closed: `today` ·
`today, partly` · `specified, not shipped` · `designed` · `not planned`. A preview page cites the
normative rule IDs that govern the thing it describes and links the spec, so "not shipped" never
means "not decided".

**The gap is the content, not the omission.** The full ladder is authored up front rather than
grown as features land, because the shape of the platform is the reason to evaluate it, and
because stub URLs that move later are worse than stub pages that fill in.

### The gate had to be inverted, not deleted

`scripts/check-docs.py` was built on the premise that `docs/` is published. Its third check read
the `exclude:` globs out of `docusaurus.config.js` and forbade a published doc from
relative-linking into an excluded one — the rule that exists because the `v0.4.0` site deploy
died on exactly that while the gate stayed green.

After the split there is no `exclude:` key. `site_excludes()` would have returned `[]` and the
check would have returned `[]` with it: **green because it stopped looking.** The new failure mode
is the mirror image — a site page relative-linking into the now-unpublished `docs/` — so the rule
is restated against the new arrangement: no page under `website/docs/` may relative-link out of
the published tree. `onBrokenMarkdownLinks` was also raised from `warn` to `throw`, since the
pages this site most wants to cite are exactly the ones it can no longer route to.

## Alternatives considered

**Keep the specs published, drop the rest.** Tempting: the specs are normative, cite RFCs by
section, and are genuinely useful to anyone integrating against this platform. Rejected because
it leaves the site with two voices and two audiences, and because the specs are more useful
beside the code that implements them. They are one GitHub link away and the site says so.

**Only document what ships today.** Smallest true surface, nothing to keep in sync. Rejected
because the ladder *is* the pitch — a reader evaluating a clustered proxy needs to see the
clustering story even when it is unbuilt, provided it is unmistakably marked.

**Grow the IA as features land.** Rejected: it churns URLs. A page that moves from
`/docs/scaling` to `/docs/operate/scaling` breaks every link anyone made to it.

**Generate the site from `docs/`.** This is what was already happening, and it is what produced
the problem.

## Risks & open questions

- **Two trees drift.** The mitigation is that the site never restates a normative rule, it cites
  one; where it must carry a fact (an exit code, a flag) that fact is checked by running the
  command. `DX-9` proposes gating this rather than trusting it.
- **Preview pages age into lies.** A section marked `specified, not shipped` is correct until the
  day it ships. Closing an `AF`, `RT`, `ME` or `KO` story must include the site page that claims
  it does not exist yet.
- **`CF-11` overlaps.** It gates that every published doc is reachable from the site, and its note
  names two unreachable specs. After the split no spec is published at all, so the concern moves
  to authored pages — `DX-9`. `CF-11` needs re-aiming or closing as superseded; it is deliberately
  left alone here rather than silently repurposed.
- **Migration pages and the provenance rule.** Naming the systems this platform competes with is
  permitted — they are not on the denylist, and non-negotiable #1 bans prior art as *rationale*,
  not as a migration target. The prose constraint stands: map concepts, never cite another
  system's behaviour as a reason for a decision here.

## Acceptance / done

- Nothing under `docs/` is routable on the published site.
- `check-docs.py` fails when a page under `website/docs/` relative-links into `docs/`, proved by a
  test that makes the link.
- Every route in `website/sidebars.js` resolves, and every page is reachable from it.
- Every command shown on the site has been executed, not just written.
- The ladder is complete from "what is this" to autoscaling, with unshipped sections marked in
  the sidebar and in their own text.
