import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { verifySourceFiles } from './source-fingerprint.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const names = new Set([
  'am-camera',
  'am-vision',
  'am-vision-types',
  'am-sdk-abi',
  'am-sdk-support',
  'am-camera-ffi',
  'am-vision-ffi',
]);
const host = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)?.[1];
if (!host) throw new Error('无法识别当前 Rust 工具链平台');
const metadata = JSON.parse(
  execFileSync(
    'cargo',
    ['metadata', '--format-version', '1', '--locked', '--offline', '--filter-platform', host],
    { cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 },
  ),
);
const packages = new Map(metadata.packages.map((item) => [item.id, item]));
const nodes = new Map(metadata.resolve.nodes.map((item) => [item.id, item]));
const roots = metadata.packages.filter((item) => names.has(item.name));
if (roots.length !== names.size) throw new Error('SDK 七个核心包不完整');
const visited = new Set();
function visit(id) {
  if (visited.has(id)) return;
  visited.add(id);
  const item = packages.get(id);
  if (!item) throw new Error(`依赖不存在：${id}`);
  if (item.source === null) {
    const relative = path.relative(root, item.manifest_path);
    if (!names.has(item.name) || relative.startsWith('..') || path.isAbsolute(relative))
      throw new Error(`SDK 引用了业务包或仓外路径：${item.name}`);
  }
  for (const dependency of nodes.get(id)?.dependencies ?? []) visit(dependency);
}
for (const item of roots) visit(item.id);

const checkedFiles = await verifySourceFiles(root);
console.log(
  JSON.stringify({
    sdkPackages: roots.map((item) => item.name),
    businessDependencies: 0,
    externalLocalPaths: 0,
    checkedSourceFiles: checkedFiles,
  }),
);
