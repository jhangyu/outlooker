## full gate on HEAD 9bd59bf (after F5)
- npm run build: PASS RC=0
- npm test: FAIL RC=1 (1 failed/71 passed): src/app/AppFrame.test.tsx > data-folder move > shows a failed move's error after the database is back in the old folder: Unable to find an accessible element with the role button and name Close
- npx vitest run src/app src/components/mail src/screens/search: PASS RC=0 (35 tests, includes src/app)
- overall: FAIL (npm test), not rerun
