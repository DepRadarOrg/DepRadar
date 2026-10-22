# DepRadar Dashboard

Read-only web dashboard for the `deprader_router` contract. It lists every
bounty and shows each bounty's claims, reading directly from Stellar
testnet. No wallet is needed, and nothing is signed or submitted.

Stack: React 19 + TypeScript + Vite, `@stellar/stellar-sdk` for contract
reads, Vitest + Testing Library for tests, oxlint for linting.

## Run it

```bash
cd frontend
npm ci
npm run dev        # http://localhost:5173
```

The dashboard defaults to the live v2 testnet contract. The contract ID
lives in `src/config.ts` and can be overridden without touching code by
copying `.env.example` to `.env.local` and editing it. After deploying a
new contract, follow [DEPLOYMENT.md](../DEPLOYMENT.md) to update the ID
here and in the other consumers.

Set `VITE_SITE_URL` to the public URL the dashboard is served from so the
Open Graph / Twitter card tags in `index.html` point at an absolute
`og-image.png` (link-preview crawlers ignore relative image URLs).

The bounty page shows the typosquat scanner's verdict for the bounty's
package. The scanner is the Rust scorer compiled to WebAssembly; run
`npm run build:wasm` (needs `wasm-pack` and the `wasm32-unknown-unknown`
target) before `npm run dev` or `npm run build` to include it. Without it
the panel says the scanner isn't available and everything else works.

## Scripts

| Command | What it does |
|---|---|
| `npm run dev` | Dev server with hot reload |
| `npm run typecheck` | `tsc -b` |
| `npm run lint` | oxlint (CI fails on any warning) |
| `npm test` | Vitest unit and component tests |
| `npm run build` | Production build to `dist/` |
| `npm run build:wasm` | Builds the typosquat scanner (`crates/typosquat-wasm`) into `public/wasm/` with [`wasm-pack`](https://rustwasm.github.io/wasm-pack/) |

## Layout

```
src/
  config.ts           network + contract ID (env-overridable), explorer links
  lib/types.ts        Bounty / Claim types mirroring the contract structs
  lib/contract.ts     all contract reads; parseBounty / parseClaim
  lib/format.ts       amounts (7 decimals), addresses, dates
  lib/insights.ts     board stats, bounty timeline, user-facing state labels
  lib/useAsync.ts     loading / error / success state for async reads
  lib/errors.ts       plain-language messages for common RPC failures
  lib/typosquat.ts    loads the wasm typosquat scanner; package from issue_ref
  components/         Layout, Radar, BountyCard, ErrorBoundary, ScannerDetection, ui (badges, identicons, skeletons, states)
  pages/              BountyList (/), BountyDetail (/bounty/:id)
```

## Conventions

- **All contract access goes through `lib/contract.ts`.** Pages never call
  the SDK directly, and contract fields are converted to camelCase types
  there.
- **Every async view handles all three states** (loading, error with retry,
  and empty) using the shared components in `components/ui.tsx`.
- **Styles use the CSS tokens** in `index.css`, with no hard-coded colors.
  The palette comes from `assets/banner.svg`: deep navy with radar teal,
  plus green (paid/accepted), amber (under review) and red (rejected),
  which are reserved for status. The UI is dark-only, by design.
- **Type:** Space Grotesk for headings and numbers, Inter for body text,
  and JetBrains Mono for addresses, IDs and code. All three are
  self-hosted via `@fontsource`, so there are no external font requests.
- **Motion is decoration only.** It must respect `prefers-reduced-motion`,
  which is already handled globally in `index.css`.
- **Show friendly state names** (`Paid`, `Refunded`, `Under review`) using
  the label maps in `lib/insights.ts`, never the raw contract enum.
- **Tests sit next to the code** (`*.test.ts(x)`). New logic in `lib/`
  needs unit tests, and new components need at least one render test.
