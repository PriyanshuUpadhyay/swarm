// An empty block with a comment that says why is allowed.
export function remove(path: string, unlink: (p: string) => void) {
  try {
    unlink(path);
  } catch {
    // the file may already be gone, and that is the state we want
  }
}
