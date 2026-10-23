# Deliverable D report — Import screen

Files (src/screens/import/): index.tsx (rewritten), SidePanel.tsx (new), syncState.ts (new), syncState.test.ts (new), import.test.tsx (edited); src/locales/{en,zh-TW}/import.json (rewritten, 86 identical keys).

## Mockup element → location
- Path DataGrid: index.tsx:140-250 (name+mono path, size, messages, +new, last sync, next sync, status, Switch, actions)
- Badges/captions per state: index.tsx:20-31, 181-210 (queued / syncing+ProgressBar+phase / locked / failed+failureReason / cancelled / disabled; dim row :323)
- Add PST path: index.tsx:272; Sync all: :276; per-row Sync now :228-238; Remove :241-250 (RemoveDialog)
- Empty state: index.tsx:289
- Schedule card: SidePanel.tsx:55-160 (radios :88-115, SpinButton+Dropdown :94-110, time Input :120, weekday ToggleButtons :125, note :148, Save/Revert)
- Activity card: SidePanel.tsx:162-245; error log card (importErrors): :247-288
- State/schedule logic: syncState.ts

## Deviations
- Native `<input type="time">` (Fluent Input): the timepicker package would be a new dependency.
- No relative times; next sync = server nextRunAt (hidden in off mode); no live preview while editing.
- Run summary lines (2/5 paths, skipped, +N) omitted: SyncStatus has no per-run counters. Card shows lastRunAt + live running/queued/locked.
- Error log is a third side card (mockup has no pane for it), shown when the selected row has errors or failed.
- Fixed 360px side panel, no splitter (Splitter/splits untouched). Panel stays visible in empty state.
- Column widths key renamed `import-paths`.
- Remove disabled while running/cancelling/removing; queued rows removable (as mockup).
