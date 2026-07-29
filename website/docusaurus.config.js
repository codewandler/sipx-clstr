// @ts-check

// The public documentation site. This is the end-user view of sipx-clstr: what it does, how to
// run it, and what it deliberately does not do yet. The internal contributor material — the story
// board, the roadmap, design records and the normative specs under ../docs — is *not* published
// here, and pages link into GitHub to reach it. See ../docs/README.md for the split, and
// scripts/check-docs.py, which fails the gate if a page on this site relative-links into ../docs.
//
// Built on every push and pull request; deployed to GitHub Pages from a published release.

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
    // or angle bracket in prose is a JSX expression to MDX, and a bare <br> is an unclosed JSX
    // tag rather than HTML.
    mermaid: true,
    hooks: {
      // Same standard as onBrokenLinks. A page that points at a doc which does not exist is the
      // failure this site is most likely to ship, because the pages it wants to cite mostly live
      // in ../docs and are not published.
      onBrokenMarkdownLinks: 'throw',
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
          // Authored for this site, in this directory. Not a view of ../docs.
          path: 'docs',
          routeBasePath: 'docs',
          sidebarPath: require.resolve('./sidebars.js'),
          editUrl: 'https://github.com/codewandler/sipx-clstr/tree/main/website/',
        },
        blog: false,
        theme: {
          customCss: require.resolve('./src/css/custom.css'),
        },
      },
    ],
  ],

  themes: [
    // The clustering pages explain topology with mermaid charts; without this they render as
    // code blocks on the pages whose whole job is to be looked at.
    '@docusaurus/theme-mermaid',
    [
      require.resolve('@easyops-cn/docusaurus-search-local'),
      {
        // Offline, index-based search — no external service to depend on or pay for.
        hashed: true,
        indexBlog: false,
        docsDir: 'docs',
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
        { to: '/docs/getting-started', label: 'Getting started', position: 'left' },
        { to: '/docs/migrate/from-kamailio', label: 'Migrate', position: 'left' },
        { to: '/docs/reference/cli', label: 'Reference', position: 'left' },
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
            { label: 'What sipx-clstr is', to: '/docs/' },
            { label: 'Getting started', to: '/docs/getting-started' },
            { label: 'Does this fit?', to: '/docs/guides/does-this-fit' },
          ],
        },
        {
          title: 'Reference',
          items: [
            { label: 'CLI', to: '/docs/reference/cli' },
            { label: 'Configuration', to: '/docs/reference/configuration' },
            { label: 'Conformance', to: '/docs/reference/conformance' },
          ],
        },
        {
          title: 'Project',
          items: [
            { label: 'GitHub', href: 'https://github.com/codewandler/sipx-clstr' },
            { label: 'sipx (the kernel)', href: 'https://github.com/codewandler/sipx' },
            {
              label: 'Specifications',
              href: 'https://github.com/codewandler/sipx-clstr/tree/main/docs/specs',
            },
            {
              label: 'Changelog',
              href: 'https://github.com/codewandler/sipx-clstr/blob/main/CHANGELOG.md',
            },
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
