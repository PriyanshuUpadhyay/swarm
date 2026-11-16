export function first(rows: number[][]) {
  for (const row of rows) {
    if (row.length) {
      for (const cell of row) {
        if (cell > 0) {
          if (cell % 2) { // expect
            return cell;
          }
        }
      }
    }
  }
  return 0;
}
