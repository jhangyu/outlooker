/** i18next's test pseudo-language "cimode" is not a BCP 47 tag; Intl would throw. */
const loc = (lng: string) => (lng === "cimode" ? "en" : lng);
const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

export function formatNumber(n: number, lng: string): string {
  return new Intl.NumberFormat(loc(lng)).format(n);
}

export function formatBytes(n: number, lng: string): string {
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 ? 0 : 1;
  const text = new Intl.NumberFormat(loc(lng), { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(value);
  return `${text} ${UNITS[unit]}`;
}

export function formatDateTime(iso: string | null, lng: string): string {
  return iso ? new Intl.DateTimeFormat(loc(lng), { dateStyle: "medium", timeStyle: "short" }).format(new Date(iso)) : "";
}

export function formatDay(iso: string | null, lng: string): string {
  return iso ? new Intl.DateTimeFormat(loc(lng), { dateStyle: "short" }).format(new Date(iso)) : "";
}
