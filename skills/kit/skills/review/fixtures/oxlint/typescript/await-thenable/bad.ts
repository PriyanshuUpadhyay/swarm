function save(): number {
  return 1;
}

export async function reference() {
  await save; // expect
}

export async function syncIterable() {
  for await (const x of [1, 2]) { // expect
    console.log(x);
  }
}
