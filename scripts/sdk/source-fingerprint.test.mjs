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

test('修订保留原清单并拒绝错基线、错前指纹、漏文件和未登记漂移', async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'framebraid-revision-'));
  const directory = path.join(root, 'crates');
  const source = path.join(directory, 'fixture.txt');
  const added = path.join(directory, 'added.txt');
  const manifest = path.join(root, 'sdk-source-manifest.json');
  const revision = path.join(root, 'sdk-source-revisions.json');
  const hash = (value) => createHash('sha256').update(value).digest('hex');
  try {
    await fs.mkdir(directory);
    const original = JSON.stringify({ files: [{ path: 'crates/fixture.txt', sha256: hash('old') }] });
    await fs.writeFile(manifest, original);
    await fs.writeFile(source, 'fixed');
    const data = {
      formatVersion: 1, baseManifestSha256: hash(original), reviewBaseCommit: 'a'.repeat(40),
      files: [{ path: 'crates/fixture.txt', previousSha256: hash('old'), sha256: hash('fixed'), reason: '修复' },
        { path: 'crates/added.txt', previousSha256: null, sha256: hash('new'), reason: '修复配套文件' }],
    };
    const save = () => fs.writeFile(revision, JSON.stringify(data));
    await save();
    await assert.rejects(verifySourceFiles(root), { code: 'ENOENT' });
    await fs.writeFile(added, 'new');
    assert.equal(await verifySourceFiles(root), 2);
    assert.equal(await fs.readFile(manifest, 'utf8'), original);
    data.baseManifestSha256 = hash('wrong');
    await save();
    await assert.rejects(verifySourceFiles(root), /未绑定/);
    data.baseManifestSha256 = hash(original);
    data.files[0].previousSha256 = hash('wrong');
    await save();
    await assert.rejects(verifySourceFiles(root), /前指纹不一致/);
    data.files[0].previousSha256 = hash('old');
    data.files.push(data.files[0]);
    await save();
    await assert.rejects(verifySourceFiles(root), /无效或重复/);
    data.files.pop();
    await save();
    await fs.writeFile(source, 'unreviewed');
    await assert.rejects(verifySourceFiles(root), /偏离已登记修订/);
  } finally {
    for (const filename of [source, added, manifest, revision])
      await fs.unlink(filename).catch((error) => { if (error.code !== 'ENOENT') throw error; });
    await fs.rmdir(directory);
    await fs.rmdir(root);
  }
});
