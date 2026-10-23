# Contract: HTML sanitize skeleton allow-list + reading-pane fonts

Frozen 2026-10-23. Only the user can change it. Round budget: 3.

## End state
Imported mail HTML keeps only the content skeleton (paragraphs, lists, inline emphasis, links, images, tables, minimal color), with no Outlook div nesting or layout styles, and the reading pane shows it in a sans font with monospace for code.

## In scope
1. `src-tauri/crates/outlooker-core/src/importer/normalize/sanitize.rs`
   - TAGS becomes exactly: p br hr blockquote pre h1 h2 h3 h4 h5 h6 ul ol li dl dt dd b strong i em u s del strike sub sup code a img table thead tbody tfoot tr td th caption font div
     (div is kept only as an intermediate; see post-pass). Dropped tags (text kept): span center small abbr cite q ins col colgroup.
   - GENERIC_ATTRIBUTES: only `style`. TAG_ATTRIBUTES: a=[href], img=[src, alt], font=[color, size], td/th=[colspan, rowspan]. Everything else removed (title lang dir align valign width height bgcolor border cellpadding cellspacing hspace vspace nowrap face start type value span).
   - STYLE_PROPERTIES: only color, background-color, font-weight, font-style, text-decoration.
   - Monospace detection: an element whose font is a monospace font is rendered as code. Font sources to check: `style` `font-family`, `font` shorthand, all Outlook `mso-*-font-family` variants (mso-ascii-font-family, mso-hansi-font-family, mso-fareast-font-family, mso-bidi-font-family) and `mso-*-theme-font`, and `<font face>`. Monospace list: Consolas, Courier New, Courier, Cascadia Mono, Cascadia Code, Lucida Console, Menlo, Monaco, SF Mono, monospace. Preferred output is wrapping/converting into `<code>`; if ammonia cannot do it without a DOM rewrite, an equivalent marker that the reading-pane CSS renders with the mono stack is acceptable (report which).
   - Post-pass after ammonia: `<div>` removed, `</div>` becomes `<br>`; `<div><br></div>` (Outlook blank line) becomes exactly one `<br>`; no run of more than the original count of line breaks is introduced; trailing `<br>` directly before a closing block tag (`</td>`, `</li>`, `</blockquote>`, `</p>`, end of body) is dropped.
2. `src/components/mail/MailPreview.tsx`: prepend a `<style>` to srcDoc:
   body font stack `Aptos, "Segoe UI", "Microsoft JhengHei UI", "Microsoft JhengHei", -apple-system, "PingFang TC", sans-serif`;
   pre/code (and the marker, if used) stack `ui-monospace, "Cascadia Mono", Consolas, Menlo, monospace`. Sandbox attribute unchanged.
3. Golden files under `src-tauri/crates/outlooker-core/tests/golden/` updated to the new output; `plain.html` input extended (or a new golden pair) to cover: nested Outlook divs, `<div><br></div>`, span with mso-*-font-family, `<p class=MsoNormal style="margin:0;font-family:Consolas;mso-fareast-font-family:新細明體">` producing code, `<font face="Courier New" color="red">`.

## Out of scope
Re-sanitizing existing DB rows, migrations, plain-text body path, search tokenizer, any other component.

## Acceptance
A1. `cargo test --workspace sanitize` (from src-tauri) green, including the existing hostile corpus test unchanged.
A2. `cargo test --workspace --test it import_pipeline` green.
A3. `cargo clippy --workspace --all-targets -- -D warnings` clean.
A4. `npm run typecheck` clean.
A5. Golden output contains no `<div`, `<span`, `class=`, `mso-`, `font-family`, `face=`, `margin`, `width=`; contains `<code` (or the chosen marker) for the Consolas paragraph and Courier New font; blank-line div yields exactly one `<br>`.
A6. Sanitized output of the hostile corpus still has zero violations.
