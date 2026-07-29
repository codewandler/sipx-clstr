// @ts-check

// Curated on purpose: this is what a reader who does not have the repository open should see, in
// the order they should see it. Ordering lives here rather than in fourteen `sidebar_position`
// fields, so the shape of the documentation is readable in one file.
//
// The ladder runs top to bottom: what it is, a first call, the things that work today, then the
// clustering and operations material that is specified but not shipped. Status is carried in the
// category label — a section marked "(preview)" does not run yet, and every page inside it says
// so again in its own words. Internal material (stories, designs, specs, roadmap) is not
// published at all; see ../docs/README.md.

const sidebars = {
  docs: [
    'intro',
    'getting-started',
    {
      type: 'category',
      label: 'Guides',
      collapsed: false,
      description: 'Everything in this section runs today.',
      items: [
        'guides/does-this-fit',
        'guides/run-a-node',
        'guides/registrations-and-calls',
        'guides/addressing',
        'guides/docker-and-k3d',
      ],
    },
    {
      type: 'category',
      label: 'Clustering (preview)',
      collapsed: false,
      description: 'Specified and normative, but not shipped. The specs are linked from each page.',
      items: [
        'clustering/how-it-works',
        'clustering/affinity-and-flows',
        'clustering/registrar-shards',
        'clustering/trunks-and-carriers',
        'clustering/media',
      ],
    },
    {
      type: 'category',
      label: 'Operate (preview)',
      collapsed: true,
      description: 'How this is meant to be run in production, once the cluster exists.',
      items: [
        'operate/deploy',
        'operate/scaling',
        'operate/observability',
        'operate/high-availability',
      ],
    },
    {
      type: 'category',
      label: 'Migrate to sipx-clstr',
      collapsed: false,
      description: 'Concept maps, with an honest account of what does not carry over.',
      items: ['migrate/from-kamailio', 'migrate/from-asterisk'],
    },
    {
      type: 'category',
      label: 'Reference',
      collapsed: true,
      items: ['reference/cli', 'reference/configuration', 'reference/conformance'],
    },
    'whats-new',
  ],
};

module.exports = sidebars;
