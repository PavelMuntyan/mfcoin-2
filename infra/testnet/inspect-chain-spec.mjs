import fs from "node:fs";

const spec = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
const genesis = spec.genesis?.runtimeGenesis;
const runtime = genesis?.config ?? genesis?.patch;
console.log(JSON.stringify({ name: spec.name, id: spec.id, rewards: runtime?.mfcRewards }, null, 2));
