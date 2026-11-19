import { promises as fs } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

async function revisedFiles(root, originalBytes, manifest) {
  const files = new Map(manifest.files.map((item) => [item.path, item]));
  let revisions;
  try {
    revisions = JSON.parse(await fs.readFile(path.join(root, 'sdk-source-revisions.json'), 'utf8'));
  } catch (error) {
    if (error.code === 'ENOENT') return files.values();
    throw error;
  }
  if (
    revisions.formatVersion !== 1 ||
    revisions.baseManifestSha256 !== sha256(originalBytes) ||
    !/^[a-f0-9]{40}$/.test(revisions.reviewBaseCommit || '') ||
    !Array.isArray(revisions.files) || revisions.files.length === 0
  ) throw new Error('源码修订清单未绑定原始提取指纹和评审基线');
  const seen = new Set();
  for (const item of revisions.files) {
    if (
      typeof item.path !== 'string' || !item.path.startsWith('crates/') ||
      item.path.includes('\\') || path.posix.normalize(item.path) !== item.path ||
      item.path.split('/').includes('..') || seen.has(item.path) ||
      !/^[a-f0-9]{64}$/.test(item.sha256 || '') ||
      typeof item.reason !== 'string' || !item.reason.trim()
    ) throw new Error('源码修订条目无效或重复');
    seen.add(item.path);
    if ((files.get(item.path)?.sha256 ?? null) !== item.previousSha256)
      throw new Error(`源码修订的前指纹不一致：${item.path}`);
    files.set(item.path, item);
  }
  return files.values();
}

export async function verifySourceFiles(root, { exemptions = COVERAGE_EXEMPTIONS } = {}) {
  let manifest;
  let originalBytes;
  try {
    originalBytes = await fs.readFile(path.join(root, 'sdk-source-manifest.json'));
    manifest = JSON.parse(originalBytes.toString('utf8'));
  } catch (error) {
    // F12：本仓强制携带提取清单——清单缺失即失败，不再以 0 静默通过。
    if (error.code === 'ENOENT')
      throw new Error('sdk-source-manifest.json 缺失：本仓核心源码必须受提取指纹约束');
    throw error;
  }
  let checked = 0;
  const realRoot = await fs.realpath(root);
  const covered = new Set();
  for (const item of await revisedFiles(root, originalBytes, manifest)) {
    if (!item.path.startsWith('crates/')) continue;
    covered.add(item.path);
    const filename = path.resolve(realRoot, item.path);
    const relative = path.relative(realRoot, filename);
    if (relative.startsWith('..') || path.isAbsolute(relative)) throw new Error('指纹路径越界');
    const realFile = await fs.realpath(filename);
    const realRelative = path.relative(realRoot, realFile);
    if (realRelative.startsWith('..') || path.isAbsolute(realRelative))
      throw new Error('源码链接越界');
    const bytes = await fs.readFile(realFile);
    if (sha256(bytes) !== item.sha256)
      throw new Error(`核心源码与提取版本不一致（或偏离已登记修订）：${item.path}`);
    checked++;
  }
  if (checked === 0) throw new Error('导出清单没有核心源码');
  await assertControlledCoverage(realRoot, covered, exemptions);
  return checked;
}

// —— F12 受控源覆盖对账：名单校验只证明"登记过的没漂"，不证明"该登记的都
// 登记了"。反向枚举受控目录，清单/修订/豁免表三者都不覆盖的新核心文件直接
// 失败，堵住"新文件不登记不失败"的缺口。——
const CONTROLLED_ROOT = 'crates';
const CONTROLLED_EXTENSIONS = new Set(['.rs', '.toml']);

// 显式豁免表：仅收"已知历史未登记项"（提取清单定稿后新增、尚未按仓规登记，
// 由 F12 盘点如实列示——登记与否交维护者裁决，不倒追登记）。豁免不是安全
// 状态；表项失效（文件已删除或已登记）同样失败，防表腐烂。
export const COVERAGE_EXEMPTIONS = new Map([
  ['crates/am-camera/src/media/mux_blackhole_tests.rs', '测试源码（受控提取语义外，维持豁免）'],
  ['crates/camera-ffi/src/captures/requests_tests.rs', '测试源码（受控提取语义外，维持豁免）'],
  ['crates/camera-ffi/src/decoder/device_selector_tests.rs', '测试源码（受控提取语义外，维持豁免）'],
  ['crates/camera-ffi/src/frames/scale_tests.rs', '测试源码（受控提取语义外，维持豁免）'],
  ['crates/camera-ffi/src/stream/tests.rs', '测试源码（受控提取语义外，维持豁免）'],
  ['crates/vision-ffi/src/inference/backends/backends_tests.rs', '历史新增测试源码未登记（待登记裁决，不倒追）'],
  ['crates/vision-ffi/src/inference/device_selector_e2e_tests.rs', '历史新增测试源码未登记（待登记裁决，不倒追）'],
  ['crates/vision-ffi/src/inference/receipt_tests.rs', '历史新增测试源码未登记（待登记裁决，不倒追）'],
  ['crates/vision-ffi/src/inference/vocab_e2e_tests.rs', '历史新增测试源码未登记（待登记裁决，不倒追）'],
  ['crates/vision-ffi/src/inference/world_e2e_tests.rs', '历史新增测试源码未登记（待登记裁决，不倒追）'],
  ['crates/vision-ffi/src/inference/ov_remote_e2e_tests.rs', '测试源码（sdk-intel-direct I-B 真机门；受控提取语义外）'],
  ['crates/vision/src/gpu/batch_e2e_tests.rs', '历史新增测试源码未登记（待登记裁决，不倒追）'],
  ['crates/vision/src/gpu/iobinding_tests.rs', '历史新增测试源码未登记（待登记裁决，不倒追）'],
]);

async function listControlledFiles(realRoot) {
  const found = [];
  const directory = path.join(realRoot, CONTROLLED_ROOT);
  const stack = [directory];
  while (stack.length > 0) {
    const current = stack.pop();
    for (const entry of await fs.readdir(current, { withFileTypes: true })) {
      // 符号链接目录不入栈（防枚举越出受控树；已登记文件的链接越界另由
      // realpath 检查拦截）；符号链接文件按其路径计入，登记与否同规则。
      if (entry.isDirectory() && !entry.isSymbolicLink()) {
        if (entry.name === 'target') continue;
        stack.push(path.join(current, entry.name));
      } else if (
        !entry.isDirectory() &&
        CONTROLLED_EXTENSIONS.has(path.extname(entry.name))
      ) {
        found.push(path.relative(realRoot, path.join(current, entry.name)).split(path.sep).join('/'));
      }
    }
  }
  return found;
}

async function assertControlledCoverage(realRoot, covered, exemptions) {
  const present = new Set(await listControlledFiles(realRoot));
  const uncovered = [...present].filter((rel) => !covered.has(rel));
  for (const rel of uncovered) {
    if (!exemptions.has(rel))
      throw new Error(`受控源未登记指纹也不在豁免表：${rel}（新核心文件须按仓规登记或评审入表）`);
  }
  for (const rel of exemptions.keys()) {
    if (covered.has(rel))
      throw new Error(`豁免表项已登记指纹，应从豁免表移除：${rel}`);
    if (!present.has(rel))
      throw new Error(`豁免表项对应文件不存在（已删除/改名应同步移除表项）：${rel}`);
  }
}
