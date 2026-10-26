import { promises as fs } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const artifacts = [
  'sdk/include/am_camera.h',
  'sdk/include/am_vision.h',
  'sdk/include/am_sdk_types.h',
  'sdk/abi/constants.json',
  'sdk/abi/defaults.json',
  'sdk/abi/layout-win-x64.json',
  'sdk/abi/decode-hardware-report-v1.schema.json',
  'crates/vision/resources/profiles.json',
  'docs/design/dual-sdk-abi-freeze-v1.1.md',
];
export async function freezeAbi({ check = false } = {}) {
  const files = [];
  for (const name of artifacts) {
    const data = await fs.readFile(path.join(root, name));
    files.push({
      path: name,
      sha256: createHash('sha256').update(data).digest('hex'),
      bytes: data.length,
    });
  }
  const content =
    JSON.stringify(
      {
        id: 'am-sdk-cabi-1.1-20261026-r6',
        previousFreezeSha256: createHash('sha256')
          .update(await fs.readFile(path.join(root, 'sdk/abi/history/freeze-v1-r5.json')))
          .digest('hex'),
        abiVersion: 65537,
        designFrozen: true,
        runtimeAcceptance: 'pending',
        files,
      },
      null,
      2,
    ) + '\n';
  const target = path.join(root, 'sdk/abi/freeze-v1.json');
  if (check) {
    if ((await fs.readFile(target, 'utf8')) !== content)
      throw new Error('ABI冻结工件发生漂移，须审查版本合同');
  } else await fs.writeFile(target, content);
  return { frozen: files.length, runtimeAcceptance: 'pending' };
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  freezeAbi({ check: process.argv.includes('--check') }).then(
    (r) => console.log(JSON.stringify(r)),
    (error) => {
      console.error(error.message);
      process.exitCode = 1;
    },
  );
}
