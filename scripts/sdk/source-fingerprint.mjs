import { promises as fs } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';

export async function verifySourceFiles(root) {
  let manifest;
  try {
    manifest = JSON.parse(await fs.readFile(path.join(root, 'sdk-source-manifest.json'), 'utf8'));
  } catch (error) {
    // 原仓库没有导出清单；仅此处允许缺失，核心文件缺失必须失败。
    if (error.code === 'ENOENT') return 0;
    throw error;
  }
  let checked = 0;
  const realRoot = await fs.realpath(root);
  for (const item of manifest.files) {
    if (!item.path.startsWith('crates/')) continue;
    const filename = path.resolve(realRoot, item.path);
    const relative = path.relative(realRoot, filename);
    if (relative.startsWith('..') || path.isAbsolute(relative)) throw new Error('指纹路径越界');
    const realFile = await fs.realpath(filename);
    const realRelative = path.relative(realRoot, realFile);
    if (realRelative.startsWith('..') || path.isAbsolute(realRelative))
      throw new Error('源码链接越界');
    const bytes = await fs.readFile(realFile);
    if (createHash('sha256').update(bytes).digest('hex') !== item.sha256)
      throw new Error(`核心源码与提取版本不一致：${item.path}`);
    checked++;
  }
  if (checked === 0) throw new Error('导出清单没有核心源码');
  return checked;
}
