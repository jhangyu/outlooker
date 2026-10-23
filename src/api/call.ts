import { invoke } from "@tauri-apps/api/core";
import type { AppError } from "./bindings/AppError";

export function isAppError(value: unknown): value is AppError {
  return typeof value === "object" && value !== null && typeof (value as { kind?: unknown }).kind === "string";
}

export function toAppError(e: unknown): AppError {
  return isAppError(e) ? e : { kind: "internal", detail: String(e) };
}

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toAppError(e);
  }
}
