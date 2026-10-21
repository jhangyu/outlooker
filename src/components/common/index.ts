import type { JSX, ReactNode } from "react";
import type { TFunction } from "i18next";
import type { AppError } from "../../api/bindings/AppError";

export interface EmptyStateProps {
  icon: JSX.Element;
  title: string;
  body?: string;
  action?: ReactNode;
  testId?: string;
}

export function EmptyState(_props: EmptyStateProps): JSX.Element | null {
  return null;
}

export function formatAppError(err: AppError, t: TFunction): string {
  return t(`common:error.${err.kind}`);
}

export function toAppError(e: unknown): AppError {
  return typeof e === "object" && e !== null && typeof (e as { kind?: unknown }).kind === "string"
    ? (e as AppError)
    : { kind: "internal", detail: String(e) };
}

export function formatNumber(n: number, lng: string): string {
  return n.toLocaleString(lng);
}

export function formatBytes(n: number, lng: string): string {
  return n.toLocaleString(lng);
}

export function formatDateTime(iso: string | null, _lng: string): string {
  return iso ?? "";
}

export function formatDay(iso: string | null, _lng: string): string {
  return iso ?? "";
}
