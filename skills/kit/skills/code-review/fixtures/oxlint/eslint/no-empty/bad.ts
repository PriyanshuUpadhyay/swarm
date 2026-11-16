export function remove(path: string, unlink: (p: string) => void) {
  try {
    unlink(path);
  } catch {} // expect
  if (path) {} // expect
}
