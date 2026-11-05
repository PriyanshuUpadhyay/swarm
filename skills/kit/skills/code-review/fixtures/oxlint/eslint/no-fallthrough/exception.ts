// Grouped empty cases and a `// falls through` comment are allowed.
export function grouped(x: number) {
  let y = 0;
  switch (x) {
    case 1:
    case 2:
      y = 1;
      // falls through
    case 3:
      y = 2;
      break;
  }
  return y;
}
