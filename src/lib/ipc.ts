import { invoke } from "@tauri-apps/api/core";
import { mockInvoke } from "./mock";
import type { IpcCommand, IpcCommandMap } from "./types";

const isTauri = "__TAURI_INTERNALS__" in window;
const useMockIpc = import.meta.env.DEV && !isTauri;

export function ipcInvoke<K extends IpcCommand>(
  command: K,
  args: IpcCommandMap[K]["args"],
): Promise<IpcCommandMap[K]["result"]> {
  if (useMockIpc) {
    return mockInvoke(command, args) as Promise<IpcCommandMap[K]["result"]>;
  }
  return invoke<IpcCommandMap[K]["result"]>(command, args);
}
