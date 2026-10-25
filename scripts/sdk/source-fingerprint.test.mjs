import assert from 'node:assert/strict';
import { promises as fs } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { verifySourceFiles } from './source-fingerprint.mjs';

test('缺少导出清单可以检查原仓库，缺少核心文件必须报错', async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'framebraid-fingerprint-'));
  const directory = path.join(root, 'crates');
  const source = path.join(directory, 'fixture.txt');
  const manifest = path.join(root, 'sdk-source-manifest.json');
  try {
    assert.equal(await verifySourceFiles(root), 0);
    await fs.mkdir(directory);
    const sha256 = createHash('sha256').update('original').digest('hex');
    await fs.writeFile(
      manifest,
      JSON.stringify({ files: [{ path: 'crates/fixture.txt', sha256 }] }),
    );
    await assert.rejects(verifySourceFiles(root), { code: 'ENOENT' });
    await fs.writeFile(source, 'original');
    assert.equal(await verifySourceFiles(root), 1);
    await fs.writeFile(source, 'changed');
    await assert.rejects(verifySourceFiles(root), /核心源码与提取版本不一致/);
    await fs.writeFile(
      manifest,
      JSON.stringify({ files: [{ path: 'crates/../../outside', sha256 }] }),
    );
    await assert.rejects(verifySourceFiles(root), /指纹路径越界/);
  } finally {
    for (const filename of [source, manifest])
      await fs.unlink(filename).catch((error) => {
        if (error.code !== 'ENOENT') throw error;
      });
    await fs.rmdir(directory);
    await fs.rmdir(root);
  }
});
