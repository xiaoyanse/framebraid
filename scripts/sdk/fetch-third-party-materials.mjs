import { createHash } from 'node:crypto';
import { createReadStream, createWriteStream, promises as fs } from 'node:fs';
import { pipeline } from 'node:stream/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { materialsPaths, rejectMaterialLinks } from './third-party-materials.mjs';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const output = process.argv[2];
const profileName = process.argv[3];
if (!output || !path.isAbsolute(output) || ![3, 4].includes(process.argv.length))
  throw new Error(
    '用法：node scripts/sdk/fetch-third-party-materials.mjs <源码外的绝对目录> [材料档案名]',
  );
const { lockPath } = await materialsPaths(repo, profileName);
const lock = JSON.parse(await fs.readFile(lockPath));
const resolved = path.resolve(output);
if (resolved === repo || resolved.startsWith(`${repo}${path.sep}`))
  throw new Error('第三方原始工件必须保存在仓库之外');
await rejectMaterialLinks(resolved);
await fs.mkdir(resolved, { recursive: true });
for (const artifact of lock.artifacts) {
  const filename = path.join(resolved, artifact.file);
  await rejectMaterialLinks(filename);
  try {
    await fs.access(filename);
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    if (!artifact.url || !artifact.url.startsWith('https://'))
      throw new Error(`工件必须按锁定构建记录生成后提供，不能自动下载：${artifact.file}`);
    const response = await fetch(artifact.url, { signal: AbortSignal.timeout(300_000) });
    if (!response.ok) throw new Error(`下载失败 ${response.status}：${artifact.url}`);
    // 排他写入；下载中断不覆盖已有工件，失败文件须人工核实后移除。
    await pipeline(response.body, createWriteStream(filename, { flags: 'wx' }));
  }
  const stat = await fs.lstat(filename);
  if (stat.isSymbolicLink() || !stat.isFile()) throw new Error(`拒绝非普通文件：${filename}`);
  const hash = createHash('sha256');
  for await (const bytes of createReadStream(filename)) hash.update(bytes);
  if (stat.size !== artifact.bytes || hash.digest('hex') !== artifact.sha256)
    throw new Error(`工件指纹不匹配，拒绝使用：${filename}`);
  console.log(`已验证 ${artifact.file}`);
}
for (const text of lock.texts.filter((entry) => entry.external)) {
  const artifact = lock.artifacts.find((entry) => entry.id === text.artifact);
  const target = path.resolve(resolved, text.externalPath);
  if (!target.startsWith(`${resolved}${path.sep}`)) throw new Error('许可原文路径越界');
  await rejectMaterialLinks(target);
  await fs.mkdir(path.dirname(target), { recursive: true });
  const code = `import pathlib,sys,zipfile,hashlib
archive,member,target,expected=sys.argv[1:]
data=zipfile.ZipFile(archive).read(member)
if hashlib.sha256(data).hexdigest()!=expected:
 raise ValueError('upstream license hash mismatch')
p=pathlib.Path(target)
if p.exists():
 if not p.is_file() or p.is_symlink() or p.read_bytes()!=data:
  raise ValueError('existing text differs')
else:
 with p.open('xb') as f:f.write(data)
`;
  execFileSync(
    process.env.AM_SDK_MATERIALS_PYTHON || 'python',
    ['-c', code, path.join(resolved, artifact.file), text.member, target, text.sha256],
    { windowsHide: true, stdio: 'inherit' },
  );
  console.log(`已提取并验证 ${text.path}`);
}
console.log(`固定原始工件已验证；源码闭包状态：${lock.sourceClosure.ffmpegStaticDependencies}。`);
