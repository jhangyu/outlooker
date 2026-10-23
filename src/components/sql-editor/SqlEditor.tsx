import { PostgreSQL, sql } from "@codemirror/lang-sql";
import { EditorState, Prec, StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, keymap, type DecorationSet } from "@codemirror/view";
import { makeStyles } from "@fluentui/react-components";
import { basicSetup } from "codemirror";
import { useEffect, useRef } from "react";
import { SQL_SCHEMA } from "./schema";

export interface SqlEditorProps {
  value: string;
  onChange(sql: string): void;
  onRun(): void;
  errorPosition: number | null;
  ariaLabel: string;
}

/** PostgreSQL `position` is a 1-based character (code point) offset; CodeMirror positions are UTF-16 offsets. */
export function codePointOffsetToUtf16(text: string, position1: number): number {
  let i = 0;
  for (let cp = 1; cp < position1 && i < text.length; cp++) i += (text.codePointAt(i) ?? 0) > 0xffff ? 2 : 1;
  return i;
}

type Mark = { from: number; to: number } | null;

function errorRange(doc: string, position: number | null): Mark {
  if (position === null || doc.length === 0) return null;
  let from = Math.min(codePointOffsetToUtf16(doc, position), doc.length - 1);
  const unit = doc.charCodeAt(from);
  if (from > 0 && unit >= 0xdc00 && unit <= 0xdfff) from -= 1; // never split a surrogate pair
  return { from, to: from + ((doc.codePointAt(from) ?? 0) > 0xffff ? 2 : 1) };
}

const setError = StateEffect.define<Mark>();
const errorMark = Decoration.mark({ class: "cm-sql-error" });
/** The marker maps through edits, so typing after an error never leaves it on a stale range. */
const errorField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(deco, tr) {
    let next = deco.map(tr.changes);
    for (const e of tr.effects) if (e.is(setError)) next = e.value ? Decoration.set([errorMark.range(e.value.from, e.value.to)]) : Decoration.none;
    return next;
  },
  provide: (f) => EditorView.decorations.from(f),
});

const theme = EditorView.theme({
  "&": { height: "100%", fontSize: "13px" },
  ".cm-scroller": { fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace" },
  ".cm-sql-error": { textDecoration: "underline wavy #c50f1f", textUnderlineOffset: "3px" },
});

const useStyles = makeStyles({ host: { height: "100%", minHeight: 0, overflow: "hidden" } });

/** In-house CodeMirror 6 wrapper (Part 01 §2: no @uiw/react-codemirror). */
export function SqlEditor({ value, onChange, onRun, errorPosition, ariaLabel }: SqlEditorProps) {
  const s = useStyles();
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const onRunRef = useRef(onRun);
  const onChangeRef = useRef(onChange);
  useEffect(() => {
    onRunRef.current = onRun;
    onChangeRef.current = onChange;
  });

  useEffect(() => {
    const v = new EditorView({
      parent: host.current as HTMLDivElement,
      state: EditorState.create({
        doc: value,
        extensions: [
          basicSetup,
          sql({ dialect: PostgreSQL, schema: SQL_SCHEMA, upperCaseKeywords: true }),
          Prec.highest(keymap.of([{ key: "Mod-Enter", run: () => (onRunRef.current(), true) }])),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) onChangeRef.current(u.state.doc.toString());
          }),
          EditorView.contentAttributes.of({ "aria-label": ariaLabel, "data-testid": "sql-editor" }),
          theme,
          errorField,
        ],
      }),
    });
    view.current = v;
    return () => {
      v.destroy();
      view.current = null;
    };
    // The view is created once per mount; `value` and `errorPosition` are synced by the effects below.
  }, [ariaLabel]);

  useEffect(() => {
    const v = view.current;
    if (!v) return;
    const doc = v.state.doc.toString();
    if (doc !== value) v.dispatch({ changes: { from: 0, to: doc.length, insert: value } });
  }, [value]);

  useEffect(() => {
    const v = view.current;
    if (!v) return;
    v.dispatch({ effects: setError.of(errorRange(v.state.doc.toString(), errorPosition)) });
  }, [errorPosition, value]);

  return <div ref={host} className={s.host} />;
}
