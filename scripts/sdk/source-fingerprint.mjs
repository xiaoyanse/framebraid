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

export async function verifySourceFiles(root) {
  let manifest;
  let originalBytes;
  try {
    originalBytes = await fs.readFile(path.join(root, 'sdk-source-manifest.json'));
    manifest = JSON.parse(originalBytes.toString('utf8'));
  } catch (error) {
    // 原仓库没有导出清单；仅此处允许缺失，核心文件缺失必须失败。
    if (error.code === 'ENOENT') return 0;
    throw error;
  }
  let checked = 0;
  const realRoot = await fs.realpath(root);
  for (const item of await revisedFiles(root, originalBytes, manifest)) {
    if (!item.path.startsWith('crates/')) continue;
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
  return checked;
}
