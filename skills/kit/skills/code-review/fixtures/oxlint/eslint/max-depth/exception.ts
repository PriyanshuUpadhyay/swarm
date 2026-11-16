// Depth counts per function, so a nested function starts again at zero.
export function first(rows: number[][]) {
  for (const row of rows) {
    if (row.length) {
      return row.find((cell) => {
        if (cell > 0) {
          if (cell % 2) {
            return true;
          }
        }
        return false;
      });
    }
  }
  return undefined;
}
