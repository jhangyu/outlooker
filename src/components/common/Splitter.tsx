import { makeStyles, mergeClasses, tokens } from "@fluentui/react-components";
import { useRef, type CSSProperties, type KeyboardEvent, type PointerEvent } from "react";
import { useTranslation } from "react-i18next";

const STEP = 16;

const useStyles = makeStyles({
  base: {
    position: "absolute", zIndex: 1, touchAction: "none", outlineStyle: "none",
    ":focus-visible": { backgroundColor: tokens.colorBrandStroke1 },
  },
  vertical: { width: "6px", cursor: "col-resize" },
  horizontal: { height: "6px", cursor: "row-resize" },
});

export type SplitterProps = {
  /** "vertical" is a vertical line between side-by-side panes (col-resize); "horizontal" separates stacked panes. */
  orientation: "vertical" | "horizontal";
  /** Size in px of the pane before the border; undefined while it follows its content. */
  value: number | undefined;
  min: number;
  /** Hard maximum; default is the parent element's size minus `minOther`. */
  max?: number;
  minOther?: number;
  onChange: (px: number) => void;
  onReset: () => void;
  /** The sized pane is AFTER the border (right/bottom pane): dragging toward the start grows it. */
  reverse?: boolean;
  /** Current px size of the sized pane, needed only while `value` is undefined (fr or auto tracks). */
  measure?: () => number;
  /** Position inside the (position: relative) parent, e.g. left/top and the grid area. */
  style?: CSSProperties;
  label?: string;
};

/**
 * Invisible 6 px hit area. Place it as a child of the layout container (`position: relative`), centred on the border.
 * Drag (pointer capture, `reverse` for a pane after the border), arrows move 16 px, Home/End jump to min/max, double-click resets.
 */
export function Splitter({ orientation, value, min, max, minOther = 0, onChange, onReset, reverse, measure, style, label }: SplitterProps) {
  const s = useStyles();
  const { t } = useTranslation("common");
  const vertical = orientation === "vertical";
  const drag = useRef<{ from: number; start: number } | null>(null);

  const limit = (el: HTMLElement) => {
    const parent = el.parentElement;
    const container = parent ? (vertical ? parent.clientWidth : parent.clientHeight) : Infinity;
    return Math.max(min, max ?? container - minOther);
  };
  const dir = reverse ? -1 : 1;
  const base = () => value ?? measure?.() ?? min;
  const apply = (el: HTMLElement, px: number) => onChange(Math.min(Math.max(px, min), limit(el)));

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { from: vertical ? e.clientX : e.clientY, start: base() };
  };
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    if (drag.current) apply(e.currentTarget, drag.current.start + dir * ((vertical ? e.clientX : e.clientY) - drag.current.from));
  };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const [dec, inc] = vertical ? ["ArrowLeft", "ArrowRight"] : ["ArrowUp", "ArrowDown"];
    const el = e.currentTarget;
    if (e.key === dec) apply(el, base() - dir * STEP);
    else if (e.key === inc) apply(el, base() + dir * STEP);
    else if (e.key === "Home") apply(el, reverse ? limit(el) : min);
    else if (e.key === "End") apply(el, reverse ? min : limit(el));
    else return;
    e.preventDefault();
  };

  return (
    <div
      role="separator"
      tabIndex={0}
      aria-label={label ?? t("action.resize")}
      aria-orientation={orientation}
      aria-valuenow={value}
      aria-valuemin={min}
      aria-valuemax={max}
      className={mergeClasses(s.base, vertical ? s.vertical : s.horizontal)}
      style={style}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={() => (drag.current = null)}
      onPointerCancel={() => (drag.current = null)}
      onDoubleClick={onReset}
      onKeyDown={onKeyDown}
    />
  );
}
