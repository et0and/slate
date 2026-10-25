import { appendFileSync } from "node:fs";

const [tag, version] = process.argv.slice(2);

if (!tag || !version) {
  throw new Error("Expected a release tag and version");
}

const output = process.env.GITHUB_OUTPUT;
if (!output) {
  console.log(`Release: ${tag} (${version})`);
  process.exit(0);
}

appendFileSync(output, `released=true\ntag=${tag}\nversion=${version}\n`);
