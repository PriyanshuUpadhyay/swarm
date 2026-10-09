async function save(): Promise<void> {}
function handle(_e: unknown) {}

export async function awaited() {
  await save();
}

export function returned() {
  return save();
}

export function handled() {
  save().catch(handle);
  save().then(() => {}, handle);
}
