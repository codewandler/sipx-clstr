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
                <Link className="button button--primary button--lg" to="/docs/getting-started">
                  Get started
                </Link>
                <Link className="button button--secondary button--lg" to="/docs/guides/does-this-fit">
                  Does this fit?
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
            <strong>Where this actually is.</strong> One node registers users and proxies calls
            between them — that part is real, and two independent phones prove it end to end with
            audio flowing directly between them. <strong>The cluster is not built yet.</strong>{' '}
            Affinity tokens, trunks, media control and the Kubernetes operator are specified and
            normative, but unimplemented. The shipped binary is also an <strong>open registrar</strong>{' '}
            whose bindings live in memory, because the configuration surface is three flags. If you
            need a clustered proxy in production today, this is not it yet — and every page here
            says which half it is describing.
          </p>
        </section>

        <section className="container">
          <pre className="topology">{TOPOLOGY}</pre>
        </section>

        <section className="container home-grid">
          <Card title="Your first call" to="/docs/getting-started">
            Build the node, start it, and watch two users register and call each other through it —
            about five minutes, with nothing but Rust and standard-library Python.
          </Card>
          <Card title="Bind is not advertise" to="/docs/guides/addressing">
            The one flag that will actually bite you. A node refuses to start on 0.0.0.0 without
            being told where peers reach it, because "everywhere" is not an answer to that question.
          </Card>
          <Card title="State rides the message" to="/docs/clustering/how-it-works">
            Tenant, shard, media node and expiry travel in a signed opaque token in Record-Route,
            Route and Path. No dialog database to consult, so none to lose. Specified, not shipped.
          </Card>
          <Card title="Media is another cluster" to="/docs/clustering/media">
            Relays are controlled over a network protocol. No RTP is ever linked into the process
            that parses SIP, so media scales and drains independently of signalling.
          </Card>
          <Card title="Scaling on SIP-shaped signals" to="/docs/operate/scaling">
            CPU is not the constraint that binds — connections and shard ownership are. Autoscaling
            reads registrations, calls per second and in-flight transactions. Designed, not built.
          </Card>
          <Card title="Coming from an existing stack?" to="/docs/migrate/from-kamailio">
            An honest concept map of what moves across, what has no equivalent yet, and what is
            different by design.
          </Card>
        </section>
      </main>
    </Layout>
  );
}
