# PST sync — parking lot

- Test `golden_inline` already fails at base 806eae0, since the sanitizer commit 410f05f. It is not caused by this work. It blocks
  contract acceptance 5 (`cargo test --workspace` green), so the user decides: fix it in this round, or leave it as a known failure.
- Import screen deviations from mockup A1 (docs/logs/pst-sync/report-D-import-screen.md): native time input, no per-run
  counters in the activity card (SyncStatus has none), error log as a third side card, no relative times, no live
  next-sync preview, fixed 360 px side panel with no splitter.
- Round 2: the error-row dedupe has no DB index. A path with tens of thousands of error rows may need one (noted in writer.rs).
- Round 2: a message that crashes the reader before its item id is read cannot be matched by id, so later jobs still retry it.
- Round 2: the global sync lock was removed. Paths now sync concurrently under a row lock plus a recheck. The round-2 reviewer checks this.
- Round 2: unregistered paths leave the path list, so the search source filter cannot select them. Their mail is still found by all other search conditions.
- Windows phase: canonicalize on a PST that Outlook holds open is untested. If it fails, the path is stored as typed.
- Round 3: the Pause-during-probe test waits a fixed 200 ms (no hook reports "probe entered"). On a very busy machine it can pass on old code; it does not fail falsely on new code.
- Round 3: a probe stuck on a dead network share is never cancelled; its thread ends when the OS times out. "Sync now" on a path that is still busy returns success with no notice.
- Round 3 reset: a stale `postmaster.pid` left by a crash, combined with a reset that deleted `PG_VERSION` but failed partway, still blocks the next reset (reset.rs:49-51). Fix: run the stop only when `PG_VERSION` also exists.
- Round 3 reset: when `pgdata` is missing but the folder marker exists (for example deleted by hand), Retry creates an empty library without asking. Nothing is deleted. The error screen could explain this.
- Round 3 reset: "Retry rebuilds the database" end to end is proven only by a scratch run (4.6 s, over the 3 s cap). The committed test proves the refusal with a plain-file `pgdata`; the symlink escape is proven only by a scratch probe.
- Round 3 reset suggestions (red team): when the database comes from a newer app version, the dialog could say "open it with the newer version" so the user does not delete recoverable data; the "Delete and reset" button could use a danger style.
