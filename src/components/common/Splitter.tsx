import { makeStyles, mergeClasses, tokens } from "@fluentui/react-components";
import { useRef, type CSSProperties, type KeyboardEvent, type PointerEvent } from "react";
import { useTranslation } from "react-i18next";

const STEP = 16;

// An absolutely positioned grid child's `auto` end line is the grid's padding edge, not the next line (CSS Grid
// "abspos items"), so a bare start line (`gridColumn: 2`) would stretch the box to the far edge of the grid and put
// the hit area at the grid's edge. Pin a bare line to one track.
const oneTrack = (v: CSSProperties["gridColumn"]) => (v === undefined || String(v).includes("/") ? v : `${v} / span 1`);

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
  /** Current maximum (already bounded by the useSplit host), undefined until the host is measured. */
  max?: number;
  /** `live` = mid-drag update (at most one per frame); the final value of a drag and every key press come without it. */
  onChange: (px: number, live?: boolean) => void;
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
 * Invisible 6 px hit area, input only. Place it in any `position: relative` element, centred on the border (it may sit
 * inside the pane it sizes: the bound comes from the useSplit host, not from where the splitter is).
 * Drag (pointer capture, `reverse` for a pane after the border; live updates once per frame, final value on release),
 * arrows move 16 px, Home/End jump to min/max, double-click resets.
 */
export function Splitter({ orientation, value, min, max, onChange, onReset, reverse, measure, style, label }: SplitterProps) {
  const s = useStyles();
  const { t } = useTranslation("common");
  const vertical = orientation === "vertical";
  // `px` = latest pointer position not yet applied, `frame` = the pending animation frame that applies it.
  const drag = useRef<{ from: number; start: number; px?: number; frame?: number } | null>(null);

  const dir = reverse ? -1 : 1;
  const base = () => value ?? measure?.() ?? min;
  const apply = (px: number, live?: boolean) => onChange(Math.min(Math.max(px, min), max ?? Infinity), live);

  const onPointerDown = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault(); // no text selection or native drag starts from the handle
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { from: vertical ? e.clientX : e.clientY, start: base() };
  };
  // Pointer moves can outpace frames (WebKit does not align them to frames): re-render the screen once per frame.
  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (!d) return;
    d.px = d.start + dir * ((vertical ? e.clientX : e.clientY) - d.from);
    d.frame ??= requestAnimationFrame(() => {
      d.frame = undefined;
      if (d.px !== undefined) apply(d.px, true);
    });
  };
  // Pointer up, cancel and lost capture all end the drag; only the first one commits the last position.
  const end = () => {
    const d = drag.current;
    drag.current = null;
    if (!d) return;
    if (d.frame !== undefined) cancelAnimationFrame(d.frame);
    if (d.px !== undefined) apply(d.px);
  };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const [dec, inc] = vertical ? ["ArrowLeft", "ArrowRight"] : ["ArrowUp", "ArrowDown"];
    if (e.key === dec) apply(base() - dir * STEP);
    else if (e.key === inc) apply(base() + dir * STEP);
    else if (e.key === "Home") apply(min);
    else if (e.key === "End" && max !== undefined) apply(max);
    else return;
    e.preventDefault();
  };

  return (
    <div
      role="separator"
      tabIndex={0}
      aria-label={label ?? t("action.resize")}
      aria-orientation={orientation}
      aria-valuenow={Math.round(value ?? (measure?.() || min))}
      aria-valuemin={min}
      aria-valuemax={max ?? min}
      className={mergeClasses(s.base, vertical ? s.vertical : s.horizontal)}
      style={style && { ...style, gridColumn: oneTrack(style.gridColumn), gridRow: oneTrack(style.gridRow) }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={end}
      onPointerCancel={end}
      onLostPointerCapture={end}
      onDoubleClick={onReset}
      onKeyDown={onKeyDown}
    />
  );
}
