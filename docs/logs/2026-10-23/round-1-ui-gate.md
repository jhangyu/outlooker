## full gate on HEAD 25d8e6b (after review fixes 3dfe7e5, 73e2d15)
- npm run build: PASS RC=0
- npm test: PASS RC=0 (71 tests)
- npx vitest run src/app src/components/mail src/screens/search: PASS RC=0 (34 tests)
- grep onEvent src/screens/search/index.tsx: PASS no match (grep RC=1)
- overall: PASS
