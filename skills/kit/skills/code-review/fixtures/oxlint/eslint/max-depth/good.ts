export function first(rows: number[][]) {
  for (const row of rows) {
    for (const cell of row) {
      if (cell > 0 && cell % 2) {
        return cell;
      }
    }
  }
  return 0;
}
