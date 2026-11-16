import assert from 'node:assert/strict';
import { promises as fs } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { verifySourceFiles } from './source-fingerprint.mjs';

// 前两组用例不测覆盖对账：注入空豁免表，避免默认活表在临时目录触发失效检查。
const verifyWithoutCoverage = (root) => verifySourceFiles(root, { exemptions: new Map() });

test('缺少导出清单必须失败（本仓强制清单），缺少核心文件必须报错', async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'framebraid-fingerprint-'));
  const directory = path.join(root, 'crates');
  const source = path.join(directory, 'fixture.txt');
  const manifest = path.join(root, 'sdk-source-manifest.json');
  try {
    // F12：清单缺失不再返回 0，直接失败。
    await assert.rejects(verifyWithoutCoverage(root), /sdk-source-manifest.json 缺失/);
    await fs.mkdir(directory);
    const sha256 = createHash('sha256').update('original').digest('hex');
    await fs.writeFile(
      manifest,
      JSON.stringify({ files: [{ path: 'crates/fixture.txt', sha256 }] }),
    );
    await assert.rejects(verifyWithoutCoverage(root), { code: 'ENOENT' });
    await fs.writeFile(source, 'original');
    assert.equal(await verifyWithoutCoverage(root), 1);
    await fs.writeFile(source, 'changed');
    await assert.rejects(verifyWithoutCoverage(root), /核心源码与提取版本不一致/);
    await fs.writeFile(
      manifest,
      JSON.stringify({ files: [{ path: 'crates/../../outside', sha256 }] }),
    );
    await assert.rejects(verifyWithoutCoverage(root), /指纹路径越界/);
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
    await assert.rejects(verifyWithoutCoverage(root), { code: 'ENOENT' });
    await fs.writeFile(added, 'new');
    assert.equal(await verifyWithoutCoverage(root), 2);
    assert.equal(await fs.readFile(manifest, 'utf8'), original);
    data.baseManifestSha256 = hash('wrong');
    await save();
    await assert.rejects(verifyWithoutCoverage(root), /未绑定/);
    data.baseManifestSha256 = hash(original);
    data.files[0].previousSha256 = hash('wrong');
    await save();
    await assert.rejects(verifyWithoutCoverage(root), /前指纹不一致/);
    data.files[0].previousSha256 = hash('old');
    data.files.push(data.files[0]);
    await save();
    await assert.rejects(verifyWithoutCoverage(root), /无效或重复/);
    data.files.pop();
    await save();
    await fs.writeFile(source, 'unreviewed');
    await assert.rejects(verifyWithoutCoverage(root), /偏离已登记修订/);
  } finally {
    for (const filename of [source, added, manifest, revision])
      await fs.unlink(filename).catch((error) => { if (error.code !== 'ENOENT') throw error; });
    await fs.rmdir(directory);
    await fs.rmdir(root);
  }
});

test('受控源覆盖对账：未登记新文件报错，豁免命中放行、表项失效报错', async () => {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'framebraid-coverage-'));
  const directory = path.join(root, 'crates');
  const registered = path.join(directory, 'registered.rs');
  const unregistered = path.join(directory, 'sneaky.rs');
  const manifest = path.join(root, 'sdk-source-manifest.json');
  const exemptions = new Map([['crates/sneaky.rs', '豁免测试']]);
  try {
    await fs.mkdir(directory);
    await fs.writeFile(registered, 'fn main() {}\n');
    const sha256 = createHash('sha256').update('fn main() {}\n').digest('hex');
    await fs.writeFile(
      manifest,
      JSON.stringify({ files: [{ path: 'crates/registered.rs', sha256 }] }),
    );
    // F12 RED：清单外的受控新文件必须失败（旧实现此处静默通过）。
    await fs.writeFile(unregistered, 'fn hidden() {}\n');
    await assert.rejects(verifyWithoutCoverage(root), /受控源未登记指纹也不在豁免表/);
    // 豁免命中放行。
    assert.equal(await verifySourceFiles(root, { exemptions }), 1);
    // 表项失效一：文件已删除未移除表项。
    await fs.unlink(unregistered);
    await assert.rejects(
      verifySourceFiles(root, { exemptions }),
      /豁免表项对应文件不存在/,
    );
    // 表项失效二：豁免路径已被登记（清单覆盖）仍留在表内。
    await fs.writeFile(unregistered, 'fn hidden() {}\n');
    const withBoth = JSON.stringify({
      files: [
        { path: 'crates/registered.rs', sha256 },
        { path: 'crates/sneaky.rs', sha256: createHash('sha256').update('fn hidden() {}\n').digest('hex') },
      ],
    });
    await fs.writeFile(manifest, withBoth);
    await assert.rejects(verifySourceFiles(root, { exemptions }), /豁免表项已登记指纹/);
  } finally {
    for (const filename of [registered, unregistered, manifest])
      await fs.unlink(filename).catch((error) => { if (error.code !== 'ENOENT') throw error; });
    await fs.rmdir(directory).catch(() => {});
    await fs.rmdir(root).catch(() => {});
  }
});
