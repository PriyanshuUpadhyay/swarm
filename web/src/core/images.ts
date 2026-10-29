/** The pasted image paths in a message: files that `/api/chat/image` saved under `uploads`. */
export function imagePaths(text: string): string[] {
  return [...new Set(text.match(/\/\S+\/uploads\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.(?:png|jpg|gif|webp)/g) ?? [])];
}

/** Pending markers hold the insertion position while the owner keeps editing. */
export const hasPendingImage = (text: string) => /\[\[Uploading image [^\]]+\]\]/.test(text);
export const insertPendingImage = (text: string, start: number, end: number, marker: string) => `${text.slice(0, start)} ${marker} ${text.slice(end)}`;
export const finishPendingImage = (text: string, marker: string, path: string) => text.replace(marker, () => path);
