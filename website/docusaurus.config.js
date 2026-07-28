// @ts-check

// The site is a *view* of `docs/`, not a copy of it: the docs plugin reads `../docs` directly so
// there is one set of words. Anything not meant for the public — the story board, the archive — is
// excluded here rather than duplicated elsewhere.

const config = {
  title: 'sipx-clstr',
  tagline: 'A clustered SIP proxy and registrar that behaves like one correct proxy.',
  url: 'https://codewandler.github.io',
  baseUrl: '/sipx-clstr/',
  organizationName: 'codewandler',
  projectName: 'sipx-clstr',
  deploymentBranch: 'gh-pages',
  trailingSlash: false,
  favicon: 'img/logo.svg',

  // A broken link on a published page is a defect, not a warning.
  onBrokenLinks: 'throw',
  markdown: {
    // Full MDX, so mermaid fences render as diagrams. That costs discipline in the docs: a brace
    // or angle bracket in prose is a JSX expression to MDX. The only four in the corpus were set
    // notations in a spec table whose neighbours were already code-spanned, so code-spanning them
    // matched the file's own convention rather than bending it for the renderer.
    mermaid: true,
    hooks: {
      onBrokenMarkdownLinks: 'warn',
    },
  },

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          path: '../docs',
          routeBasePath: 'docs',
          sidebarPath: require.resolve('./sidebars.js'),
          // The board and the archive are working material: generated, churny, and meaningless
          // without the repo around them.
          exclude: ['stories/**', 'archive/**', 'README.md'],
          editUrl: 'https://github.com/codewandler/sipx-clstr/tree/main/',
        },
        blog: false,
        theme: {
          customCss: require.resolve('./src/css/custom.css'),
        },
      },
    ],
  ],

  themes: [
    // architecture.md carries six mermaid charts; without this they render as code blocks on the
    // one page whose whole job is to be looked at.
    '@docusaurus/theme-mermaid',
    [
      require.resolve('@easyops-cn/docusaurus-search-local'),
      {
        // Offline, index-based search — no external service to depend on or pay for.
        hashed: true,
        indexBlog: false,
        // The docs live outside the website directory; without this the indexer looks for
        // `website/docs`, finds nothing, and ships a search box that returns no results.
        docsDir: '../docs',
        docsRouteBasePath: 'docs',
        highlightSearchTermsOnTargetPage: true,
      },
    ],
  ],

  themeConfig: {
    image: 'img/logo.svg',
    colorMode: {
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'sipx-clstr',
      logo: {
        alt: '',
        src: 'img/logo.svg',
      },
      items: [
        { type: 'docSidebar', sidebarId: 'docs', position: 'left', label: 'Docs' },
        { to: '/docs/vision', label: 'Vision', position: 'left' },
        { to: '/docs/specs/proxy-behavior', label: 'Specs', position: 'left' },
        { to: '/docs/roadmap', label: 'Roadmap', position: 'left' },
        {
          href: 'https://github.com/codewandler/sipx-clstr',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Start here',
          items: [
            { label: 'Vision', to: '/docs/vision' },
            { label: 'Architecture', to: '/docs/architecture' },
            { label: 'Roadmap', to: '/docs/roadmap' },
          ],
        },
        {
          title: 'Specifications',
          items: [
            { label: 'Proxy behaviour', to: '/docs/specs/proxy-behavior' },
            { label: 'Location service', to: '/docs/specs/location-service' },
            { label: 'Affinity token', to: '/docs/specs/affinity-token' },
            { label: 'Hook framework', to: '/docs/specs/hook-framework' },
          ],
        },
        {
          title: 'More',
          items: [
            { label: 'GitHub', href: 'https://github.com/codewandler/sipx-clstr' },
            { label: 'sipx (the kernel)', href: 'https://github.com/codewandler/sipx' },
            { label: 'Upstream ledger', to: '/docs/upstream' },
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} Codewandler. MIT or Apache-2.0.`,
    },
    prism: {
      additionalLanguages: ['bash', 'toml', 'rust', 'yaml', 'ini'],
    },
  },
};

module.exports = config;
