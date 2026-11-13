import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { useCallback, useEffect, useRef, useState } from "react";

export type AppUpdatePhase =
  | "idle"
  | "checking"
  | "current"
  | "available"
  | "downloading"
  | "installing"
  | "error";

export type AppUpdateState = {
  phase: AppUpdatePhase;
  currentVersion: string | null;
  availableVersion: string | null;
  notes: string | null;
  publishedAt: string | null;
  downloadedBytes: number;
  totalBytes: number | null;
  error: string | null;
};

const initialState: AppUpdateState = {
  phase: "idle",
  currentVersion: null,
  availableVersion: null,
  notes: null,
  publishedAt: null,
  downloadedBytes: 0,
  totalBytes: null,
  error: null,
};

export function useAppUpdater() {
  const [state, setState] = useState(initialState);
  const update = useRef<Update | null>(null);
  const checking = useRef(false);

  useEffect(() => {
    let active = true;
    void getVersion()
      .then((version) => {
        if (active) {
          setState((current) => ({ ...current, currentVersion: version }));
        }
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);

  const checkForUpdates = useCallback(async () => {
    if (checking.current) return;
    checking.current = true;
    setState((current) => ({
      ...current,
      phase: "checking",
      error: null,
    }));
    try {
      const next = await check({ timeout: 30_000 });
      const previous = update.current;
      update.current = next;
      if (previous && previous !== next) await previous.close();
      if (!next) {
        setState((current) => ({
          ...current,
          phase: "current",
          availableVersion: null,
          notes: null,
          publishedAt: null,
          downloadedBytes: 0,
          totalBytes: null,
          error: null,
        }));
        return;
      }
      setState({
        phase: "available",
        currentVersion: next.currentVersion,
        availableVersion: next.version,
        notes: next.body?.trim() || null,
        publishedAt: next.date ?? null,
        downloadedBytes: 0,
        totalBytes: null,
        error: null,
      });
    } catch (error) {
      setState((current) => ({
        ...current,
        phase: update.current ? "available" : "error",
        error: `Could not check for updates: ${String(error)}`,
      }));
    } finally {
      checking.current = false;
    }
  }, []);

  const installUpdate = useCallback(async () => {
    const pending = update.current;
    if (!pending) {
      setState((current) => ({
        ...current,
        phase: "error",
        error: "Check for updates again before installing.",
      }));
      return;
    }
    setState((current) => ({
      ...current,
      phase: "downloading",
      downloadedBytes: 0,
      totalBytes: null,
      error: null,
    }));
    try {
      await pending.download((event) => {
        if (event.event === "Started") {
          setState((current) => ({
            ...current,
            totalBytes: event.data.contentLength ?? null,
          }));
        } else if (event.event === "Progress") {
          setState((current) => ({
            ...current,
            downloadedBytes: current.downloadedBytes + event.data.chunkLength,
          }));
        }
      });
      await invoke("prepare_for_update");
      setState((current) => ({ ...current, phase: "installing" }));
      await pending.install({ restartAfterInstall: true });
    } catch (error) {
      setState((current) => ({
        ...current,
        phase: "available",
        error: `Could not install the update: ${String(error)}`,
      }));
    }
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => void checkForUpdates(), 3_000);
    return () => window.clearTimeout(timer);
  }, [checkForUpdates]);

  useEffect(
    () => () => {
      const pending = update.current;
      update.current = null;
      if (pending) void pending.close();
    },
    [],
  );

  return { state, checkForUpdates, installUpdate };
}
