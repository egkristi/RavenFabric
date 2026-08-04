# ravenfabric.io

Source for [ravenfabric.io](https://ravenfabric.io) — the project landing page for [RavenFabric](https://github.com/egkristi/RavenFabric).

## Stack

- **Static HTML/CSS** — inlined CSS, zero JS dependencies
- **Cloudflare Pages** — builds directly from GitHub (connected repo)
- **Custom domain** — `ravenfabric.io` (DNS configured in Cloudflare dashboard)
- **No build step** — edit files directly, no bundlers or frameworks

## Local preview

```bash
# Serve with Python's built-in server for proper testing
python3 -m http.server 8000
# then visit http://localhost:8000
```

## Deployment

Auto-deploys on push to `main` via Cloudflare Pages:

```text
git push origin main
  ↓
Cloudflare Pages builds from repo
  ↓
Live at https://ravenfabric.io within ~1-2 minutes
```

## Structure

```text
.
├── index.html              # Main single-page landing (hero, features, architecture, FAQ, etc.)
├── _headers                # Cloudflare Pages security headers (CSP, HSTS, etc.)
├── wrangler.toml            # Cloudflare Pages configuration
├── robots.txt              # SEO crawler directives
├── sitemap.xml             # Sitemap for search engines (8 URLs)
├── feed.xml                # RSS feed for blog
├── install.sh              # One-line installer script (curl | sh)
├── 404.html                # Custom 404 page
├── .well-known/
│   └── security.txt        # RFC 9116 security contact
├── blog/
│   ├── index.html           # Blog listing (4 posts)
│   ├── ai-guardrails.html
│   ├── demo-multi-node-ubuntu.html
│   ├── noise-xx-deep-dive.html
│   └── why-noise-xx-over-tls.html
├── demos/
│   └── index.html           # Live demos page (11 demos with recordings)
└── assets/
    ├── favicon.svg          # SVG favicon
    ├── og-image.svg         # Open Graph source
    ├── og-image.png         # Open Graph rendered (1200×630)
    ├── og-image.webp        # Open Graph WebP variant
    ├── architecture.svg     # 7-layer architecture diagram
    ├── fonts/               # Self-hosted fonts (IBM Plex Sans/Serif, JetBrains Mono)
    └── demos/               # 10 SVG architecture diagrams for demo page
```

## Maintenance Rules

- **No JavaScript** — keep the site static HTML/CSS only
- **No localhost references** — CI validates no `localhost` or `127.0.0.1` in HTML
- **Security headers** — defined in `website/_headers`, served natively by Cloudflare Pages

## License

Site content: same as RavenFabric — AGPLv3 + Commercial.
