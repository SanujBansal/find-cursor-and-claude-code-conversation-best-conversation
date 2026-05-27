"use client";

import { useSyncExternalStore } from "react";
import { isTauriRuntime } from "./tauri";

function subscribe() {
  return () => {};
}

/**
 * Detect Tauri without hydration mismatch (server snapshot is always false).
 */
export function useTauriRuntime(): boolean {
  return useSyncExternalStore(
    subscribe,
    () => isTauriRuntime(),
    () => false,
  );
}
