import { invoke } from "@tauri-apps/api/core";
import type { AppError } from "./bindings/AppError";

export function isAppError(value: unknown): value is AppError {
  return typeof value === "object" && value !== null && typeof (value as { kind?: unknown }).kind === "string";
}

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    if (isAppError(e)) throw e;
    const wrapped: AppError = { kind: "internal", detail: String(e) };
    throw wrapped;
  }
}
