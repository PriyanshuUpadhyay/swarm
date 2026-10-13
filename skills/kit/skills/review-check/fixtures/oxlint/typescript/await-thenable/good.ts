async function save(): Promise<number> {
  return 1;
}

async function* numbers() {
  yield 1;
}

export async function called() {
  await save();
  for await (const x of numbers()) {
    console.log(x);
  }
}
