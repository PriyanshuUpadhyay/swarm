export function next(x: number) {
  let y = 0;
  switch (x) {
    case 1:
      y = 1;
    case 2: // expect
      y = 2;
      break;
  }
  return y;
}
