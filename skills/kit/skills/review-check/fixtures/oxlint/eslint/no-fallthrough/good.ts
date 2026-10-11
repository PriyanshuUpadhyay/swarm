export function ended(x: number) {
  switch (x) {
    case 1:
      return 1;
    case 2:
      throw new Error("two");
    case 3: {
      break;
    }
    default:
      return 0;
  }
  return 4;
}
