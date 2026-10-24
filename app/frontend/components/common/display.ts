/** Single-line text that truncates with "…". Spread into a makeStyles block: `title: { ...ellipsis }`. */
export const ellipsis = { overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", minWidth: 0 } as const;

/** i18next's test pseudo-language "cimode" is not a BCP 47 tag; Intl would throw. */
const loc = (lng: string) => (lng === "cimode" ? "en" : lng);
const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

const cache = new Map<string, Intl.NumberFormat | Intl.DateTimeFormat>();
function formatter(kind: "n", lng: string, o?: Intl.NumberFormatOptions): Intl.NumberFormat;
function formatter(kind: "d", lng: string, o?: Intl.DateTimeFormatOptions): Intl.DateTimeFormat;
function formatter(kind: "n" | "d", lng: string, o?: object) {
  const key = `${kind}|${loc(lng)}|${JSON.stringify(o)}`;
  let f = cache.get(key);
  if (!f) {
    f = kind === "n" ? new Intl.NumberFormat(loc(lng), o) : new Intl.DateTimeFormat(loc(lng), o);
    cache.set(key, f);
  }
  return f;
}

export function formatNumber(n: number, lng: string): string {
  return formatter("n", lng).format(n);
}

export function formatBytes(n: number, lng: string): string {
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 ? 0 : 1;
  const text = formatter("n", lng, { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(value);
  return `${text} ${UNITS[unit]}`;
}

export function formatDateTime(iso: string | null, lng: string): string {
  return iso ? formatter("d", lng, { dateStyle: "medium", timeStyle: "short" }).format(new Date(iso)) : "";
}

export function formatDay(iso: string | null, lng: string): string {
  return iso ? formatter("d", lng, { dateStyle: "short" }).format(new Date(iso)) : "";
}

export function formatMonth(iso: string, lng: string): string {
  return formatter("d", lng, { year: "numeric", month: "long" }).format(new Date(iso));
}
