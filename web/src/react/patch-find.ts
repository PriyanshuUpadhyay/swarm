// Chat Find reaches a line inside a Pierre patch through this lookup, so the transcript does not
// load Pierre. Each mounted patch box registers a finder under its preview id.
const finders = new Map<string, (line: number) => Element | null>();

export function addFinder(previewId: string, find: (line: number) => Element | null): () => void {
  finders.set(previewId, find);
  return () => {
    if (finders.get(previewId) === find) finders.delete(previewId);
  };
}

/** The drawn row for display line `line` of a preview's patch, if that row is on screen. */
export const findLine = (previewId: string, line: number): Element | null => finders.get(previewId)?.(line) ?? null;
