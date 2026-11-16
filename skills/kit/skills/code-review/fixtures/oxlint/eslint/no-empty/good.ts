export function remove(path: string, unlink: (p: string) => void) {
  try {
    unlink(path);
  } catch (err) {
    throw new Error(`could not remove ${path}`, { cause: err });
  }
}
