# Round 1 review fix F1 (blocker) — owner r1-ui-impl-1-sonnet

ROLE: WORKER — you may NOT spawn agents, teams, or workflows. If the task needs delegation, STOP and report back to leader.

Source: `tmp/review/round-1-review.md` §F1; repro `tmp/review/refetch.test.tsx`.

## Problem
With `<Activity>`, re-showing Search refetches every loaded `searchMail` page and `searchCount` (default staleTime 0). The screen's own `library:changed` listener (`search/index.tsx:108-120`) is torn down while hidden, so the new-mail bar is missed. This violates spec Part 03 §9 (no reload of the current list; show a "new mail, refresh" bar).

## Decided fix
- (a) `staleTime: Infinity` on the `searchMail` infinite query (`pages.ts:38`) and the `searchCount` query (`search/index.tsx:91`). Results change only through `seq` or `refresh()`.
- (b) Delete the screen's `library:changed` listener, `showingRef` and `newMail` state. Derive the bar from `useAppStatus().counts`, which `boot.ts:26` updates globally. Store a `{messages, psts}` baseline whenever `seq` advances, and re-baseline in `refresh()`. Show the bar when `query !== null` and the current counts differ from the baseline.
- Key `filterOptions` as `["filterOptions", status.counts.messages, status.counts.psts]` and drop its invalidation.
- Other screens are already checked. No other screen listens to events. Their refetch-on-show (`pstList`, `dbStatus`, `savedQueries`) is wanted, so leave it.

## Files (exclusive)
`src/screens/search/index.tsx`, `src/components/mail/pages.ts`, `src/screens/search/search.test.tsx`, `src/app/AppFrame.test.tsx`.

## Acceptance
1. Add a new regression test based on the probe: search "invoice" → nav Import → emit `library:changed` → nav Search. Expect `searchMail` and `searchCount` call counts to be unchanged and `new-mail-bar` to be visible. Report its red output on the current HEAD first, then green.
2. The existing new-mail bar and refresh tests stay green, with no assertion weakened.
3. `grep -n "onEvent" src/screens/search/index.tsx` → no match.
4. `mise exec -- npm run build` and `mise exec -- npm test` → RC=0 locally. The final gate is ordered by the architect.

## Git red lines
No stash (including stash list), reset, `checkout --`, clean, or `add -A`. Use `git commit -- <own files>` only. Do not put `;` in `-m`.

## Report to r1-ui-architect-opus
Commit hash, conclusions with file:line, files changed, acceptance pass/fail, uncertain items. Last line `READY_FOR_SIGNOFF` or `BLOCKED`.
