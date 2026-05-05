# ravenfabric.io

Source for [ravenfabric.io](https://ravenfabric.io) — the project landing page for [RavenFabric](https://github.com/egkristi/RavenFabric).

## Stack

- **Static HTML/CSS** — single `index.html` with inlined CSS, zero JS dependencies
- **GitHub Pages** — hosting
- **GitHub Actions** — auto-deploy on push to `main`
- **Custom domain** — `ravenfabric.io` (CNAME)

## Local preview

The site is a single static file with no build step. Just open it:

```bash
# macOS
open index.html

# Linux
xdg-open index.html

# Or serve with Python's built-in server for proper testing
python3 -m http.server 8000
# then visit http://localhost:8000
```

## Deployment

Pushes to `main` automatically deploy via `.github/workflows/deploy.yml`:

```
git push origin main
  ↓
GitHub Actions: build (validate HTML, upload artifact)
  ↓
GitHub Actions: deploy to GitHub Pages
  ↓
Live at https://ravenfabric.io within ~1-2 minutes
```

## Structure

```
.
├── index.html              # Single-page landing
├── CNAME                   # Custom domain for GitHub Pages
├── robots.txt              # SEO crawler directives
├── sitemap.xml             # Sitemap for search engines
├── .well-known/
│   └── security.txt        # RFC 9116 security contact
├── assets/
│   ├── favicon.svg         # Inline SVG favicon
│   ├── og-image.svg        # Open Graph source
│   └── og-image.png        # Open Graph rendered (1200×630)
└── .github/workflows/
    └── deploy.yml          # GitHub Pages deploy pipeline
```

## License

Site content: same as RavenFabric — AGPLv3 + Commercial.
