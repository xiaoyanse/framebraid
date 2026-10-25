import { promises as fs } from 'node:fs';
import { createHash } from 'node:crypto';
import path from 'node:path';

// 构建配置来自显式记录并核二进制，不从目录名猜测Release。
export async function verifyBuildRecord(filename, dllDir) {
  const record = JSON.parse(await fs.readFile(filename, 'utf8'));
  if (
    record.schemaVersion !== 1 ||
    record.configuration !== 'release' ||
    record.exitCode !== 0 ||
    !/^[a-f0-9]{40}$/.test(record.sourceCommit || '') ||
    !Array.isArray(record.command) ||
    record.command[0] !== 'cargo' ||
    record.command[1] !== 'build' ||
    !record.command.includes('--release') ||
    !record.command.includes('--locked')
  )
    throw new Error('Release构建记录无效：必须是成功的锁定优化构建');
  const expected = new Set(['am_camera.dll', 'am_vision.dll']);
  if (!Array.isArray(record.binaries) || record.binaries.length !== expected.size)
    throw new Error('Release构建记录必须包含两份SDK DLL');
  for (const row of record.binaries) {
    if (!expected.delete(row.file) || !/^[a-f0-9]{64}$/.test(row.sha256 || ''))
      throw new Error('Release二进制清单无效');
    const bytes = await fs.readFile(path.join(dllDir, row.file));
    if (
      bytes.length !== row.bytes ||
      createHash('sha256').update(bytes).digest('hex') !== row.sha256
    )
      throw new Error(`Release构建记录与实际DLL不匹配：${row.file}`);
  }
  return record;
}
