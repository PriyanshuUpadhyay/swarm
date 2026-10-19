declare const maybe: number | Promise<number>;

// A union that can hold a promise may be awaited.
export async function union() {
  return await maybe;
}
