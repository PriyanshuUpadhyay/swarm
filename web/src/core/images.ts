/** The pasted image paths in a message: files that `/api/chat/image` saved under `uploads`. */
export function imagePaths(text: string): string[] {
  return [...new Set(text.match(/\/\S+\/uploads\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.(?:png|jpg|gif|webp)/g) ?? [])];
}
