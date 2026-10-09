async function save(): Promise<void> {}

// `void` passes the tool, so TS-33 (a seat ID) checks the reason and the error path.
export function fireAndForget() {
  void save();
}
