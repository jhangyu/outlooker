# PST sync — parking lot

- Test `golden_inline` already fails at base 806eae0, since the sanitizer commit 410f05f. It is not caused by this work. It blocks
  contract acceptance 5 (`cargo test --workspace` green), so the user decides: fix it in this round, or leave it as a known failure.
- Import screen deviations from mockup A1 (docs/logs/pst-sync/report-D-import-screen.md): native time input, no per-run
  counters in the activity card (SyncStatus has none), error log as a third side card, no relative times, no live
  next-sync preview, fixed 360 px side panel with no splitter.
