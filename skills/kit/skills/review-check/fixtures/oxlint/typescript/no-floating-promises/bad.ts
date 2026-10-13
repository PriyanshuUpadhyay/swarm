async function save(): Promise<void> {}

export function bare() {
  save(); // expect
}

export class Store {
  constructor() {
    save(); // expect
  }
}

export function noHandler() {
  save().catch(); // expect
  save().then(() => {}); // expect
}
