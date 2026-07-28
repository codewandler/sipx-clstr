import React from 'react';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import useBaseUrl from '@docusaurus/useBaseUrl';

const TOPOLOGY = `     carriers / trunks / UAs
              │
   DNS NAPTR/SRV + source-preserving VIP
              │
   ┌──────────┴──────────┐     any edge can serve any request:
   │  edge   edge   edge │◄─┐  the signed token in the Route header
   └──────────┬──────────┘  │  says where the dialog belongs
              │             │
    registrar shards ───────┘  one owner per AoR, by rendezvous hash
              │
      media relays — a cluster of its own`;

function Card({title, children, to}) {
  return (
    <Link className="home-card" to={to}>
      <h2>{title}</h2>
      <p>{children}</p>
    </Link>
  );
}

export default function Home() {
  return (
    <Layout
      title="A clustered SIP proxy and registrar"
      description="Many nodes, one observable behaviour — a clustered SIP proxy and registrar that is indistinguishable from a single correct proxy.">
      <main>
        <section className="home-hero">
          <div className="container home-hero-inner">
            <div>
              <p className="eyebrow">clustered SIP proxy &amp; registrar</p>
              <h1>sipx-clstr</h1>
              <p className="hero-copy">
                Run one SIP proxy and its behaviour is RFC 3261's. Run five and something changes
                that no RFC describes. sipx-clstr keeps <strong>no shared call state</strong>: every
                mid-dialog request carries what it needs in a signed token, so any healthy node can
                route it — and the cluster stays indistinguishable from one correct proxy.
              </p>
              <div className="hero-actions">
                <Link className="button button--primary button--lg" to="/docs/vision">
                  Read the vision
                </Link>
                <Link className="button button--secondary button--lg" to="/docs/specs/proxy-behavior">
                  Browse the specs
                </Link>
              </div>
            </div>
            <div className="hero-art">
              <img src={useBaseUrl('img/logo.svg')} alt="" width="340" height="340" />
            </div>
          </div>
        </section>

        <section className="container" style={{paddingTop: '2.5rem'}}>
          <p className="status-note">
            <strong>Where this actually is.</strong> sipx-clstr is early. Four load-bearing
            specifications are written and cross-reconciled — proxy behaviour, location service,
            affinity token, hook framework — and the Cargo workspace now exists with its gate green,
            but <strong>nothing forwards a SIP message yet</strong>. M1, which makes one node proxy
            and register, is scoped as fourteen ordered stories with exit criteria you can run. If
            you need a proxy today, this is not it. If you want to read the argument before the
            implementation, it is all here, and that is deliberate.
          </p>
        </section>

        <section className="container">
          <pre className="topology">{TOPOLOGY}</pre>
        </section>

        <section className="container home-grid">
          <Card title="State rides the message" to="/docs/specs/affinity-token">
            Tenant, shard, media node and expiry travel in a signed opaque token in Record-Route,
            Route and Path. No dialog database to consult, so none to lose — proved by a cross-node
            lookup counter that must read zero.
          </Card>
          <Card title="Every resource has one owner" to="/docs/specs/location-service">
            A connection belongs to the edge that accepted it; a registration belongs to one shard by
            rendezvous hash. Changing the shard count is a drain-then-switch, never a silent rehash.
          </Card>
          <Card title="Media is another cluster" to="/docs/designs/media-control">
            Relays are controlled over a network protocol behind a trait. No RTP is ever linked into
            the process that parses SIP, so media scales and drains independently of signalling.
          </Card>
          <Card title="Deterministic before distributed" to="/docs/designs/conformance-harness">
            Every cluster behaviour reproduces in a seeded, virtual-time, multi-node simulation
            before it touches a socket. A failure is a seed you replay, not a flake you re-run.
          </Card>
          <Card title="Proxy-first" to="/docs/specs/proxy-behavior">
            Dialogs stay end-to-end between endpoints. The platform forwards, forks and record-routes
            — it does not terminate calls. A B2BUA is a separate, optional service.
          </Card>
          <Card title="Upstream first" to="/docs/upstream">
            Protocol logic lives in the sipx kernel; this repo adds orchestration. Every requirement
            that belongs upstream is recorded in a ledger rather than shadow-implemented here.
          </Card>
        </section>
      </main>
    </Layout>
  );
}
