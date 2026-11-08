/// <reference types="node" />
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";

it("has no browser parser imports or package entries", () => {
  const name = ["markdown", "it"].join("-");
  const source = new URL("../", import.meta.url);
  const importsPackage = new RegExp(`(?:\\bfrom\\s*|\\bimport\\s*(?:\\(\\s*)?|\\brequire\\s*\\(\\s*)["']${name}(?:/[^"']*)?["']`);
  const imports = readdirSync(source, { recursive: true, encoding: "utf8" })
    .filter((path) => /\.[cm]?[jt]sx?$/.test(path))
    .filter((path) => importsPackage.test(readFileSync(fileURLToPath(new URL(path, source)), "utf8")))
    .sort();
  const manifest = JSON.parse(readFileSync(new URL("../package.json", source), "utf8")) as Record<string, Record<string, unknown>>;
  const names = [name, ["@types", name].join("/")];
  const packages = ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"]
    .flatMap((section) => names.filter((key) => Object.hasOwn(manifest[section] ?? {}, key)).map((key) => `${section}/${key}`));

  expect({ imports, packages }).toEqual({ imports: [], packages: [] });
});
