import assert from 'node:assert/strict';
import { promises as fs } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { packageDelivery } from './package-delivery.mjs';

const root = path.resolve(import.meta.dirname, '../..');

test('打包只接受本仓分类子目录且不覆盖已有文件', async () => {
  await assert.rejects(packageDelivery({ output: 'relative' }), /绝对路径/);
  for (const output of [
    root,
    path.dirname(root),
    path.join(root, '.local/releases'),
    path.join(root, '.local/build/new'),
    path.join(root, 'sdk/new'),
  ]) {
    await assert.rejects(packageDelivery({ output }), /分类下/);
  }
  const parent = path.join(root, '.local/tests');
  await fs.mkdir(parent, { recursive: true });
  const directory = await fs.mkdtemp(path.join(parent, 'package-paths-'));
  const sentinel = path.join(directory, 'keep.txt');
  try {
    // 未提供输入时，先确认合法输出能通过边界检查且不会创建产物。
    await assert.rejects(packageDelivery({ output: path.join(directory, 'new') }), /dll-dir/);
    await fs.writeFile(sentinel, 'keep');
    await assert.rejects(packageDelivery({ output: directory }), /拒绝覆盖/);
    assert.equal(await fs.readFile(sentinel, 'utf8'), 'keep');
  } finally {
    await fs.unlink(sentinel).catch((error) => {
      if (error.code !== 'ENOENT') throw error;
    });
    await fs.rmdir(directory);
  }
});
