import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { mockInvoke } from "./mock";
import type { AppError } from "./types";

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const isMac =
  typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);

if (typeof document !== "undefined") {
  document.documentElement.dataset.platform = isMac ? "macos" : "other";
}

export function normalizeError(value: unknown): AppError {
  if (value && typeof value === "object") {
    const candidate = value as Partial<AppError> & { message?: unknown };
    if (typeof candidate.message === "string") {
      return {
        code: typeof candidate.code === "string" ? candidate.code : "internal",
        message: candidate.message,
        hint: typeof candidate.hint === "string" ? candidate.hint : null,
      };
    }
  }
  if (typeof value === "string") {
    return { code: "internal", message: value, hint: null };
  }
  return {
    code: "internal",
    message: "操作失败，请查看诊断信息与日志。",
    hint: null,
  };
}

export async function api<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    if (isTauri) {
      return await invoke<T>(command, args);
    }
    return (await mockInvoke(command, args)) as T;
  } catch (error) {
    throw normalizeError(error);
  }
}

export async function onEvent<T>(
  name: string,
  handler: (payload: T) => void,
): Promise<() => void> {
  if (!isTauri) {
    return () => {};
  }
  return listen<T>(name, (event) => handler(event.payload));
}
