import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import { createPortal } from "react-dom";
import { Clipboard, Copy } from "lucide-react";
import { readClipboardText, writeClipboardText } from "../lib/clipboard";
import { useHarnessStore } from "../lib/store";

type EditableElement = HTMLInputElement | HTMLTextAreaElement;

interface TextMenuState {
  x: number;
  y: number;
  target: EditableElement | null;
  selectedText: string;
  canCopy: boolean;
  canPaste: boolean;
}

function isEditable(element: EventTarget | null): element is EditableElement {
  return element instanceof HTMLTextAreaElement
    || (element instanceof HTMLInputElement && !["button", "checkbox", "radio", "range", "submit", "reset", "file", "color"].includes(element.type));
}

function replaceSelection(target: EditableElement, text: string) {
  const start = target.selectionStart ?? target.value.length;
  const end = target.selectionEnd ?? start;
  const nextValue = `${target.value.slice(0, start)}${text}${target.value.slice(end)}`;
  const prototype = target instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(prototype, "value")?.set?.call(target, nextValue);
  target.dispatchEvent(new Event("input", { bubbles: true }));
  target.focus();
  const caret = start + text.length;
  target.setSelectionRange(caret, caret);
}

export default function TextContextMenu() {
  const [menu, setMenu] = useState<TextMenuState | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const notify = useHarnessStore((state) => state.notify);

  useEffect(() => {
    const openMenu = (event: MouseEvent) => {
      event.preventDefault();
      const target = event.target;
      const editable = isEditable(target) ? target : null;
      const inputSelection = editable && editable.selectionStart !== null && editable.selectionEnd !== null
        ? editable.value.slice(editable.selectionStart, editable.selectionEnd)
        : "";
      const selectedText = editable ? inputSelection : window.getSelection()?.toString() ?? "";
      const canCopy = selectedText.length > 0 && !(editable instanceof HTMLInputElement && editable.type === "password");
      const canPaste = Boolean(editable && !editable.disabled && !editable.readOnly);
      if (!editable && !canCopy) {
        setMenu(null);
        return;
      }
      const width = 176;
      const height = 88;
      setMenu({
        target: editable,
        selectedText,
        canCopy,
        canPaste,
        x: Math.max(8, Math.min(event.clientX, window.innerWidth - width - 8)),
        y: Math.max(8, Math.min(event.clientY, window.innerHeight - height - 8)),
      });
    };
    document.addEventListener("contextmenu", openMenu);
    return () => document.removeEventListener("contextmenu", openMenu);
  }, []);

  useEffect(() => {
    if (!menu) return;
    const closeOutside = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setMenu(null);
    };
    const closeKeyboard = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMenu(null);
    };
    const closeOnScroll = () => setMenu(null);
    document.addEventListener("pointerdown", closeOutside);
    document.addEventListener("keydown", closeKeyboard);
    window.addEventListener("resize", closeOnScroll);
    document.addEventListener("scroll", closeOnScroll, true);
    menuRef.current?.querySelector<HTMLButtonElement>("button:not([disabled])")?.focus();
    return () => {
      document.removeEventListener("pointerdown", closeOutside);
      document.removeEventListener("keydown", closeKeyboard);
      window.removeEventListener("resize", closeOnScroll);
      document.removeEventListener("scroll", closeOnScroll, true);
    };
  }, [menu]);

  const copy = async () => {
    if (!menu?.canCopy) return;
    const text = menu.selectedText;
    setMenu(null);
    try {
      await writeClipboardText(text);
    } catch (error) {
      notify(`复制失败：${String(error)}`, "error");
    }
  };

  const paste = async () => {
    if (!menu?.canPaste || !menu.target) return;
    const target = menu.target;
    setMenu(null);
    try {
      const text = await readClipboardText();
      replaceSelection(target, text);
    } catch (error) {
      notify(`粘贴失败：${String(error)}`, "error");
    }
  };

  const navigate = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const items = Array.from(menuRef.current?.querySelectorAll<HTMLButtonElement>("button:not([disabled])") ?? []);
    if (items.length === 0) return;
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    const next = event.key === "Home" ? 0
      : event.key === "End" ? items.length - 1
        : event.key === "ArrowDown" ? (current + 1 + items.length) % items.length
          : (current - 1 + items.length) % items.length;
    items[next]?.focus();
  };

  if (!menu) return null;
  return createPortal(
    <div
      ref={menuRef}
      className="text-context-menu"
      role="menu"
      aria-label="文字操作"
      style={{ left: menu.x, top: menu.y }}
      onKeyDown={navigate}
      onContextMenu={(event) => { event.preventDefault(); event.stopPropagation(); }}
    >
      <button type="button" role="menuitem" disabled={!menu.canCopy} onClick={() => void copy()}>
        <Copy size={16} /><span>复制</span><kbd>Ctrl C</kbd>
      </button>
      <button type="button" role="menuitem" disabled={!menu.canPaste} onClick={() => void paste()}>
        <Clipboard size={16} /><span>粘贴</span><kbd>Ctrl V</kbd>
      </button>
    </div>,
    document.body,
  );
}
