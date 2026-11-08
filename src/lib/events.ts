import { listen } from "@tauri-apps/api/event";
import type { HarnessEventMap, HarnessEventName } from "./types";

const mockEventTarget = new EventTarget();
const isTauri = "__TAURI_INTERNALS__" in window;
const useMockEvents = import.meta.env.DEV && !isTauri;

export function emitMockEvent<K extends HarnessEventName>(
  event: K,
  payload: HarnessEventMap[K],
): void {
  mockEventTarget.dispatchEvent(new CustomEvent(event, { detail: payload }));
}

export function listenToHarnessEvent<K extends HarnessEventName>(
  event: K,
  handler: (payload: HarnessEventMap[K]) => void,
  onError?: (message: string) => void,
): () => void {
  if (useMockEvents) {
    const listener = (nativeEvent: Event) => {
      handler((nativeEvent as CustomEvent<HarnessEventMap[K]>).detail);
    };
    mockEventTarget.addEventListener(event, listener);
    return () => mockEventTarget.removeEventListener(event, listener);
  }

  let disposed = false;
  let unlisten: (() => void) | undefined;
  void listen<HarnessEventMap[K]>(event, (nativeEvent) => {
    if (!disposed) handler(nativeEvent.payload);
  }).then((dispose) => {
    if (disposed) dispose();
    else unlisten = dispose;
  }).catch((error) => {
    onError?.(`无法监听“${event}”事件：${String(error)}。请检查应用权限或重启 Harness。`);
  });

  return () => {
    disposed = true;
    unlisten?.();
  };
}
