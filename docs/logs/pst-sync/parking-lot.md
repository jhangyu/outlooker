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
