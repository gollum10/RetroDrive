type CloseRequest = { preventDefault: () => void };

/** Tauri awaits this handler, then destroys the window unless it was prevented.
 * Do not call close() here: that requests another close event.
 */
export function guardedWindowClose(flush: () => Promise<boolean>, onError: (error: unknown) => void) {
  let pending = false;
  return async (event: CloseRequest): Promise<void> => {
    if (pending) { event.preventDefault(); return; }
    pending = true;
    try {
      if (!await flush()) event.preventDefault();
    } catch (error) {
      event.preventDefault();
      onError(error);
    } finally { pending = false; }
  };
}
