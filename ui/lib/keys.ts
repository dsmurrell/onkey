/** A key pressed while typing (in a text box, a text area such as "Report a problem", a menu or anything editable),
 *  or while a dialog is open (Setup, Report a problem, the control prompt: Space there presses its focused button,
 *  never the music app's play/pause, and M never switches to Mini under Setup): the window's one-key shortcuts
 *  leave it alone. */
export function typing(e: KeyboardEvent): boolean {
  const t = e.target as Element | null;
  return !!t?.closest?.("input, textarea, select, [contenteditable]:not([contenteditable=false]), [role=dialog]")
    || !!globalThis.document?.querySelector?.("[role=dialog]");
}
