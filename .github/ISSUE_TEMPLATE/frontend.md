---
name: Frontend feature or component
about: Propose or track a new component, page, or integration in the React dashboard
title: "[frontend] "
labels: frontend
---

## Component / Page Scope

Which dashboard view or component does this issue affect?
(e.g. `BountyDetail`, `BountyList`, `Leaderboard`, `Address`, `ThemeToggle`, `WalletButton`, etc.)

## User Experience & Mockup

- **User Story:** What can the user see, configure, or interact with?
- **Layout & Design:** Note any specific design requirements, responsive breakpoints, or theme styling rules (`index.css` CSS variables).

## Contract / RPC Integration

- Which `deprader_router` getter, event, or client function does this component consume?
- Note any transaction submission flows (Freighter wallet signing, simulation, balance lookups, or claim submissions).

## Accessibility & Localization

- Screen reader semantics (ARIA labels, table captions, focus states).
- Dark and light mode contrast compliance.
- Text strings integrated with `i18n.ts` if applicable.

## Definition of Done

- [ ] Component implemented in `frontend/src/`
- [ ] Component unit tests written and passing (`npm test`)
- [ ] `npm run lint` and `npm run typecheck` pass with zero errors
- [ ] Added an entry to `PROGRESS.md` describing what was built, by whom, and which issue it closes
