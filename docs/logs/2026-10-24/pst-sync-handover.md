# PST scheduled sync, round 3 — Session Handover

> **建立時間**：2026-10-24 12:20 (Asia/Taipei)
> **交接目的**：讓下一個 session 接續 PST 定時同步的第 3 輪（最後一輪預算），終態是：第 2 輪審查的 2 個建議修正與 4 個小問題修好，新增「資料庫重設」入口，開發資料庫重設並重新產生資料，整體驗收全過，使用者完成實機檢查。
> **目前判定**：第 1、2 輪已完成並合併。第 3 輪尚未開工。使用者已裁決範圍（見 §8）。
> **可信版本錨點**：branch `main`；HEAD `df000bc`（本檔提交後會再多一個 docs commit）。驗證對象是 `df000bc`。

## 0. 接手速讀
- **目標**：使用者登錄多個 PST 路徑，依間隔或每日排程同步，只新增不刪除。Outlook 鎖住檔案時顯示「鎖定，等待中」並重試。每個路徑可以暫停／繼續、取消登錄（保留郵件）、刪除。
- **目前位置**：功能已全部在 main。第 2 輪審查判定可以合併，留下 2 個建議修正與 4 個小問題（`docs/logs/pst-sync/review-round2.md`）。
- **下一個動作**：凍結第 3 輪契約，附加到 `docs/logs/pst-sync/contract.md` 尾端（內容見 §8）。之後開 team，平行派發 §8 的 P1–P3。
- **最大風險／紅線**：不可破壞「只新增不刪除」、「暫停保留進度」、「取消登錄保留郵件」。dev 資料庫重設已獲使用者授權，**不必備份**，僅限 `scripts/tmp/demo-root`。

## 1. 接手啟動序列
1. Read `docs/logs/pst-sync/contract.md`（全文，含 round 1、round 2 與 additions）— 現行規格與驗收條件。
2. Read `docs/logs/pst-sync/review-round2.md` 第 48–125 行 — 第 3 輪要修的 6 項，含 file:line 與建議修法。
3. Read `docs/logs/pst-sync/parking-lot.md` — 已知限制。這些不是 bug，不要修。
4. Run `git -C /Users/jhangyu/project/outlooker log --oneline -1 && git -C /Users/jhangyu/project/outlooker status --porcelain` — 預期 HEAD 為本檔的 docs commit，工作樹乾淨。
5. 記憶檔 `~/.claude/projects/-Users-jhangyu-project-outlooker/memory/outlooker-test-env-traps.md` — 平行 worktree 的 3 個新陷阱寫在這裡（見 §9）。

## 2. 目的、現象與根因狀態
- **目的**：見速讀「目標」。
- **第 3 輪要處理的現象**（第 2 輪審查，皆已由審查員讀碼或探針確認）：
  1. 同一個匯入工作重跑時（暫停後繼續、重開程式、DB 暫停、游標重設），錯誤列會重複寫入，而且是永久的。探針結果 before=1 after=2。根因：去重條件排除了「本工作」的列，見 `writer.rs:291-296` 與 `pipeline.rs:215`。建議修法：`(e.job_id IS DISTINCT FROM $2 OR n.pst_item_id > 0 OR n.stage = 'scan')`。
  2. 某個路徑的檔案檢查卡住時（例如網路磁碟），之後所有路徑的排程都停住，結束程式也會卡住。根因：排程器 await 整個 `sync(All)`，見 `sync/mod.rs:123`；`stop` 會 await 這個任務，見 `sync/mod.rs:58`；`queue.rs:552-572` 會等所有路徑完成。建議修法：spawn 每次同步並加上「每個路徑同時只跑一個」的防護，`stop` 不 await 它。**這一項違反使用者明確要求「背景執行拆開、互不卡住」。**
  3. 小問題：檢查檔案期間按暫停或取消登錄，仍會排入一個永不執行的工作（`queue.rs:492-495`）。
  4. 小問題：同步中「刪除」按鈕仍可按，按下回報忙碌（`src/screens/import/index.tsx:261`）。
  5. 小問題：刪除失敗後，「重試」按鈕沒有作用（`index.tsx` 的 Sync now 按鈕，status failed 時 title 為 `row.retry`；`queue.rs:477`）。
  6. 小問題：Windows `\\?\` 前綴的長度檢查算的是位元組，不是 UTF-16 單位（`queue.rs:295`）。
- **新需求（使用者 2026-10-24）**：資料庫格式錯誤時，使用者會卡在警告畫面，完全無法使用。要提供一個入口讓使用者重設、清空資料庫並重新開始。**尚未探索**。起點：migration 失敗由 `src-tauri/crates/outlooker-core/src/db/mod.rs:88-94` 回傳 `AppError::internal("migration failed: ...")`；refinery 遇到已套用的 migration 被改動會中止。警告畫面的前端位置**未查**，先 grep `src/app` 與 `src/screens/onboarding`。

## 3. 範圍與版本控制狀態
- In scope：`src-tauri/crates/outlooker-core/src/{importer,sync,lifecycle,db}`、`src-tauri/src/commands`、`src/screens/import`、DB 錯誤畫面、`src-tauri/crates/outlooker-loadgen/src/load.rs`、`scripts/tmp/demo-root`。
- Out of scope：Windows 實測（SSH 階段另排）、VSS、鏡像同步、每路徑各自排程、系統匣與開機自動啟動。
- Branch／HEAD：`main`／`df000bc`。worktree 只剩主樹，8 個暫存 worktree 與分支都已合併並刪除。
- Working tree：只有 `docs/logs/pst-sync/review-round2.md` 未追蹤，本檔提交時會一起提交。
- 另一個 session 在同一個 repo 做分割線（splitter）相關工作，例如 commit `4c15645`。**不要動 `src/components/common/Splitter.tsx`、`splits.ts`**，除非先確認那邊已停。
- 相關 commits（本功能）：`ab4875d`、`b0c1fdd`、`7e81448`、`09222b9`（第 1 輪）；`65fa2de`、`7c0b8b9`、`984db4a`、`dcb67ec`、`698224b`、`134b7f7`、`99b488b`、`46bbd52`（第 2 輪合併）；`8d0b5ec`（圖片保留寬高）；`a9ca1d3`（release 建置警告）。
- 背景狀態：無 team 成員存活（session config 只剩 team-lead），無背景程序，無殘留 PG 叢集。

## 4. 目前邏輯架構（本階段切面）
| 節點 | 責任 | 關鍵符號 | 上游 | 下游 | 不變式／失敗語意 |
|---|---|---|---|---|---|
| 排程器 | 依排程或補跑觸發同步 | `core/src/sync/mod.rs`（`:58` stop、`:123` 觸發）、`sync/schedule.rs`（下次時間、DST，用 `time` 0.3.55 local-offset） | app 啟動（`app/mod.rs:101` 之後）| `ImportManager::sync` | 程式開著才跑。錯過排程只補跑一次。不佔 Work Gate |
| Tauri 命令 | 前端入口 | `src-tauri/src/commands/{import,sync}.rs`；27 個命令（`dto/commands.rs`）| 前端 `src/api/commands.ts` | ImportManager | 新增命令要同步更新 `dto/commands.rs`、`lib.rs`、`commands.ts`、`commands.json` |
| 同步步驟 | 每個路徑：檢查檔案 → 決定略過、鎖定、失敗或排入 | `importer/queue.rs` `sync`（`:552-572`）、`sync_one`（約 `:477-532`）| 排程器、`sync_now` | `import_jobs` 列 | 檔案 I/O 走 spawn_blocking，不持鎖。最新工作為刪除時略過。暫停或取消登錄的路徑略過 |
| 檔案檢查 | 同一份程式碼判斷各平台的鎖定 | `importer/probe.rs`（OS 錯誤碼 32/33 = 鎖定，無 cfg）| 同步步驟、register | — | 鎖定時不跑 finalize／VACUUM，不寫錯誤列，連續鎖定只留一列 |
| 登錄 | 正規化路徑，一個檔案只登錄一次 | `queue.rs` register（canonicalize、probe、metadata 在同一個 spawn_blocking，`:355-367`）| `pst_register` | `pst_sources` | `UNIQUE(path)`。重新登錄沿用舊列 |
| 執行器／管線 | 單 worker 依序跑工作，可依檢查點續跑 | `importer/runner.rs`（`:83`、`:109` 跳過已暫停）、`pipeline.rs`、`writer.rs` | queue | DB | 已知項目以 `pst_item_id` 跳過。毒項目跨工作跳過 |
| 暫停／取消登錄／刪除 | 三種路徑動作 | `queue.rs:425` set_paused、`:441` unregister、`importer/remove.rs` | 命令 | DB | 暫停保留檢查點。取消登錄保留郵件。刪除只刪該路徑獨有的郵件，排隊中的匯入工作直接刪除 |
| 匯入頁 | A1 設計稿：路徑表格、側欄（排程卡、活動卡、錯誤記錄）| `src/screens/import/{index.tsx,SidePanel.tsx,syncState.ts,RemoveDialog.tsx}` | boot 事件 `sync:status-changed`、`import:status-changed` | 命令 | 狀態由 `syncState.ts` rowState 推導 |

## 5. 資料生產消費鏈
- **Happy path**：排程器 tick → `sync(All)` → 每個路徑 `sync_one`：spawn_blocking 跑 stat 與 probe → 未變則不建工作，有變則插入 `import_jobs(queued)` → runner 取工作 → pipeline 讀 PST（跳過已知 `pst_item_id` 與毒項目）→ writer COPY 寫入（以 `dedupe_key` 跨 PST 去重）→ 發 `import:status-changed`、`sync:status-changed` → 前端 boot 更新 `["pstList"]` 快取 → 匯入頁。
- **Failure path**：檔案被鎖時寫入終態 `locked`，同一段連續鎖定只留一列。檔案不見或無法讀取時寫一筆 failed 工作與一筆 `job_failed` 錯誤列；原因改變才新增。解析錯誤寫入 `import_errors`，相同內容會把舊列移到新工作（第 3 輪要修的就是「同一工作內重複」這個缺口）。

## 6. 型別與介面契約
| 契約 | Producer | Consumer | 不變式 | 證據 |
|---|---|---|---|---|
| `PstSource` | `dto/import.rs`（`paused`、`lastCheckedAt`、`lastSyncedAt`、`failureReason`；`progress` 已 Box）| `src/api/bindings/PstSource.ts`、`syncState.ts` | 由 ts-rs 產生，禁止手寫 | 產生方式：跑 canonical `cargo test` |
| `ImportStatus` | queued／running／locked／failed／completed／removing（已無 cancelling/cancelled）| 前端 badge | 新增值時前端 badge 要一起改 | `dto/import.rs` |
| `SyncSchedule` | `{mode:"off"}` 或 `{mode:"interval",minutes}` 或 `{mode:"daily",time:"HH:MM",weekdays:[1..7]}` | `SidePanel.tsx` ScheduleCard | weekdays 不可為空且不重複，否則回 invalid | `dto/sync.rs` |
| 路徑動作命令 | `pst_set_paused(pstId,paused)->PstSource`、`pst_unregister(pstId)->null`、`pst_remove`（刪除）| 匯入頁的動作按鈕 | 命令總數 27 | `docs/logs/pst-sync/r2-actions-api.md` |
| Schema | `V1__schema.sql` 直接修改，無新 migration | refinery | 改 V1 會讓舊資料庫啟動失敗，這就是新需求「重設入口」的由來 | `db/mod.rs:88-94` |

## 7. 已完成事項
| 結果 | 驗證 | 版本錨點 |
|---|---|---|
| [C] 第 1、2 輪全部功能 | 主樹 gate log 在 `scripts/tmp/`（gitignored）：`g-lib.log`（lib 128+7 通過）、`g-it.log`（it 86 通過、1 ignored）、`g-cl.log`（cluster_lifecycle 5 通過）、`g-rel.log`（`cargo build --release` 0 個 `^warning:`）、`npm-final.log`（67 通過）、`build-final.log`；clippy 在 `99b488b` 執行，RC=0 | `46bbd52`／`df000bc`（後者只多 docs）|
| [C] 第 2 輪審查 | `docs/logs/pst-sync/review-round2.md`：可以合併，0 blocker | `df000bc` |
| [C] 圖片保留寬高、`golden_inline` 修好 | lib normalize 19 通過 | `8d0b5ec` |
| [C] release 建置零警告 | `cargo build --release` 0 warnings | `a9ca1d3` |

註：gate 的 RC 是我在回合內自己捕捉的。log 只記了輸出，沒寫入 `RC=$?`，所以依規範算「宣稱」。最終驗收時要在 main 重跑一次全部閘。

## 8. 待解議題（使用者 2026-10-24 已裁決；照此凍結第 3 輪契約）
| 優先 | 狀態 | 議題 | 下一動作 | 完成條件 |
|---|---|---|---|---|
| P0 | [D→決] | 凍結第 3 輪契約 | 把本表 P1–P4 的完成條件附加到 `contract.md`，標明「Round 3（最後一輪）」| 檔案已提交 |
| P1 | 待做 | 審查建議修正 1：同一工作內錯誤列重複 | 在 `writer.rs:291-296` 套用建議條件，修正 `pipeline.rs:215` scan 列的重寫 | 回歸測試：暫停中途、改檔、繼續後錯誤列數不變（紅燈後轉綠），≤1 s |
| P1 | 待做 | 審查建議修正 2：卡住的檢查拖住所有排程與結束程式 | `sync/mod.rs:123` 改成 spawn，加每個路徑同時只跑一個的防護；`:58` 的 stop 不 await | 測試：一個路徑的 probe 被擋住時，下一次 tick 的其他路徑照常同步，`stop` 立即返回 |
| P1 | 待做 | 4 個小問題（§2 第 3–6 項）| 依 review-round2.md 第 103–125 行逐項修 | 每項附驗證（核心邏輯才提交測試）|
| P2 | 待做，**未探索** | 新需求：資料庫格式錯誤時提供「重設資料庫」入口 | 先找警告畫面與 migration 失敗的錯誤流；設計「清空資料目錄 → 重新 initdb → 回到首次使用流程」，需二次確認對話框 | 格式錯誤時畫面上有重設按鈕。確認後資料庫清空並可正常使用。核心邏輯有一個測試（資料完整性、安全：只刪資料目錄）|
| P3 | 待做 | 重設 dev 資料庫並重新產生（使用者選 a：不必備份）| ① 改 `outlooker-loadgen/src/load.rs:43`，讓假來源寫入 `paused = true`；② 停掉 demo-root 的 PG（若在跑）後刪除 `scripts/tmp/demo-root`；③ `cargo run` loadgen `seed --data-root scripts/tmp/demo-root --messages <N>`（**N 未定**，見 §11）；④ 在 app 登錄 `tests/fixtures/external/outlook.pst` 跑一次真實同步（需實機，交給使用者，或用 P2 完成後的流程）| 匯入頁的假來源顯示「已暫停」，不是失敗。outlook.pst 同步完成 |
| P4 | 待做 | 最終驗收 | 在 main 跑：clippy、`cargo test --workspace --lib`、`--test it`、`--test cluster_lifecycle`（私有 TMPDIR）、`cargo build --release` 計 warning、`npm test`、`npm run build` | 全綠，log 內含 `RC=$?` 與 commit hash |
| P4 | 待使用者 | 實機檢查 | 交給使用者：登錄 PST → 立即同步 → 完成；改排程後下次時間更新；側欄拖拉（280–640 px、雙擊回 360、重整後保留）；暫停／繼續／取消登錄／刪除；閱讀窗格圖片不超出 | 使用者回報 |

紅隊：第 1、2 輪已用完「每個交付至多兩輪」。第 3 輪新增的 P2（重設入口，會動使用者資料）屬新交付，必須送一次 fresh 紅隊。

## 9. 嘗試、裁決與禁止重踩
| 嘗試 | 結果 | 裁決 | 可重試 |
|---|---|---|---|
| 平行 worktree 共用 `CARGO_TARGET_DIR` | `crates/pff/build.rs` 的 rerun-if-changed 用絕對路徑，libpff C 編譯（>40 s）互相觸發重跑 | 每個 worktree 用私有 target dir，先以 `cargo build --workspace`（150 s 上限）暖機 | 否 |
| 私有 target dir 冷跑 clippy | tauri 的 check 單元超過 40 s | 不申請延長，分支上不跑 clippy，改在 main 合併後跑（main 的 clippy 快取是熱的，約 3–24 s）| 否 |
| 平行 worktree 共用 TMPDIR 跑 cargo test | `test_support/template.rs:57-74` 會刪掉其他 schema key 的 PG 範本，出現 `initdb: could not open file base/1/2618` | 每個 worktree 用私有 `TMPDIR` | 否 |
| 成員對衝突的 merge 做 `git commit` 不帶路徑 | hook 擋下，並觸發斷路器 | 成員解完衝突後 `git add` 並回報。lead 確認暫存區只有合併檔，再自己 `git commit --no-edit`（使用者已授權 lead 這樣做）| 否（成員）|
| `--workspace` 的 clippy／build | 經由 feature unification 開啟 `test-support`，蓋掉只有 app 建置才會出現的警告 | 驗收另跑 `cargo build --release`，grep `^warning:` | — |
| 匯入頁原設計：停用開關、單次取消 | 已由暫停／繼續、取消登錄、刪除取代 | 不要加回 | 否 |
| 架構方案 1（新增 V3 migration）| 不採用 | 使用者選「直接改 V1，dev 資料庫重設」| 否 |

## 10. 未來方向（不阻塞）
- Windows 階段（透過 SSH，使用者開機後才做）：`arch-2.md` §3 第 1–3 步的 Outlook 鎖定實測，以及 §2 的郵件 ID 穩定性實測；canonicalize 在 Outlook 開檔時是否成功。觸發條件：macOS 側全部完成後。

## 11. 已知限制與不確定性
- **需使用者決策**：P3 假郵件的數量 N。使用者沒有給，預設建議 10,000 封。先問，或用預設並在回報中說明。
- **使用者的回覆有歧義**：使用者寫的是「1. a 3. a」，但上一則只問了問題 1 與問題 2。我把「3. a」解讀為問題 2 選 a（重新產生假郵件、假來源標成暫停、另登錄 outlook.pst）。如有疑慮，先向使用者確認。
- 已知限制：見 `docs/logs/pst-sync/parking-lot.md`（錯誤去重無索引、未讀到 ID 前就崩潰的郵件無法比對、取消登錄的路徑不在搜尋來源篩選中、改時區需重開程式、匯入頁的 4 項設計稿差異維持現狀）。
- 未驗證：Windows 上的一切行為、實機的版面與拖拉。

## 12. 驗收命令（從 `src-tauri/` 執行；TMPDIR 用私有目錄）
```bash
cargo clippy --workspace --all-targets -- -D warnings > ../scripts/tmp/r3-clippy.log 2>&1; echo RC=$? >> ../scripts/tmp/r3-clippy.log   # 預期 RC=0
TMPDIR=$PWD/../scripts/tmp/t-r3 cargo test --workspace --lib > ../scripts/tmp/r3-lib.log 2>&1; echo RC=$? >> ../scripts/tmp/r3-lib.log     # 預期 RC=0
TMPDIR=$PWD/../scripts/tmp/t-r3 cargo test --workspace --test it > ../scripts/tmp/r3-it.log 2>&1; echo RC=$? >> ../scripts/tmp/r3-it.log
TMPDIR=$PWD/../scripts/tmp/t-r3 cargo test --workspace --test cluster_lifecycle > ../scripts/tmp/r3-cl.log 2>&1; echo RC=$? >> ../scripts/tmp/r3-cl.log
cargo build --release > ../scripts/tmp/r3-rel.log 2>&1; echo RC=$? >> ../scripts/tmp/r3-rel.log; grep -c "^warning:" ../scripts/tmp/r3-rel.log   # 預期 0
cd .. && mise exec -- npm test > scripts/tmp/r3-npm.log 2>&1; echo RC=$? >> scripts/tmp/r3-npm.log && mise exec -- npm run build > scripts/tmp/r3-build.log 2>&1; echo RC=$? >> scripts/tmp/r3-build.log
```
上限：測試 40 s、建置 150 s。超時就拆 target，不放到背景執行。不傳 `-q`／`--format`。

## 13. 參考入口
- 必讀：`docs/logs/pst-sync/contract.md`、`docs/logs/pst-sync/review-round2.md:48-125`、`docs/logs/pst-sync/parking-lot.md`。
- 設計：`docs/logs/pst-sync/arch-2.md`（基底設計）、`docs/logs/pst-sync/r2-actions-api.md`、`docs/design/import-sync/A1.html`（選定的設計稿）。
- 需求：`docs/superpowers/specs/2026-10-24-pst-scheduled-sync-design.md`。
