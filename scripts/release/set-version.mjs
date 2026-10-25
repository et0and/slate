import { readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2];
const semver = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

if (!version || !semver.test(version)) {
  throw new Error(`Expected a semantic version, received: ${version ?? "(nothing)"}`);
}

function replace(path, pattern, label) {
  const source = readFileSync(path, "utf8");
  if (!pattern.test(source)) {
    throw new Error(`Could not find ${label} in ${path}`);
  }
  writeFileSync(path, source.replace(pattern, (_, before, after) => `${before}${version}${after}`));
}

replace(
  "Cargo.toml",
  /(\[workspace\.package\][\s\S]*?\nversion = ")[^"]+(")/,
  "[workspace.package] version",
);

for (const name of ["slate-core", "slate-app"]) {
  replace(
    "Cargo.lock",
    new RegExp(`(name = "${name}"\\nversion = ")[^"]+(")`),
    `the ${name} package version`,
  );
}

replace(
  "packaging/macos/Info.plist",
  /(<key>CFBundleShortVersionString<\/key>\s*<string>)[^<]+(<\/string>)/,
  "CFBundleShortVersionString",
);

console.log(`Set Slate version to ${version}`);
