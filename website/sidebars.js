// @ts-check

// Curated on purpose. `docs/` also holds the story board and working notes; what appears here is
// what a reader who does not have the repository open should see, in the order it makes sense in.

const sidebars = {
  docs: [
    'vision',
    'architecture',
    'roadmap',
    {
      type: 'category',
      label: 'Specifications',
      collapsed: false,
      description: 'The normative contracts, each with a test-vector table.',
      items: [
        'specs/proxy-behavior',
        'specs/location-service',
        'specs/affinity-token',
        'specs/hook-framework',
      ],
    },
    {
      type: 'category',
      label: 'Designs',
      collapsed: true,
      description: 'One design record per epic: why, the approach, what was rejected.',
      items: [
        'designs/proxy-engine',
        'designs/registrar-location',
        'designs/cluster-affinity',
        'designs/routing-trunks',
        'designs/media-control',
        'designs/extension-framework',
        'designs/conformance-harness',
        'designs/deployment',
        'designs/k8s-deployment-operator',
        'designs/e2e-tester',
        'designs/services-b2bua',
      ],
    },
    'upstream',
  ],
};

module.exports = sidebars;
