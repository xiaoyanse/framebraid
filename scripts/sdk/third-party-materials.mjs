import { createHash } from 'node:crypto';
import { createReadStream, promises as fs } from 'node:fs';
import path from 'node:path';

export async function rejectMaterialLinks(filename) {
  let current = path.resolve(filename);
  while (true) {
    try {
      if ((await fs.lstat(current)).isSymbolicLink()) throw new Error(`拒绝链接路径：${current}`);
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
    }
    const parent = path.dirname(current);
    if (parent === current) break;
    current = parent;
  }
}

async function sha256(filename) {
  const hash = createHash('sha256');
  for await (const bytes of createReadStream(filename)) hash.update(bytes);
  return hash.digest('hex');
}

export async function materialsPaths(repository, profileName) {
  let base = path.join(repository, 'sdk/third-party');
  try {
    await fs.access(base);
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    base = path.join(repository, 'third-party');
  }
  if (profileName && !/^[a-z0-9][a-z0-9-]{0,63}$/.test(profileName))
    throw new Error('材料档案名只允许小写字母、数字和连字符');
  const lockPath = profileName
    ? path.join(base, 'profiles', `${profileName}.json`)
    : path.join(base, 'materials-lock.json');
  await rejectMaterialLinks(lockPath);
  return { base, lockPath };
}

export function assertRuntimePolicy(policy, ffmpeg, binaryEvidence) {
  if (!policy) return;
  for (const item of ffmpeg) {
    const flags = item.configuration.split(/\s+/);
    if (item.license !== policy.expectedLicense)
      throw new Error(`候选运行库许可声明与受控档案不一致：${item.file}`);
    if (
      policy.requiredFlags.some((flag) => !flags.includes(flag)) ||
      policy.forbiddenFlags.some((flag) => flags.includes(flag)) ||
      policy.forbiddenFlagPrefixes.some((prefix) => flags.some((flag) => flag.startsWith(prefix)))
    )
      throw new Error(`候选运行库构建配置超出受控档案：${item.file}`);
  }
  for (const item of binaryEvidence.filter((binary) => binary.group === 'ffmpeg')) {
    if (
      item.directImports.some((name) => !policy.allowedDirectImports.includes(name.toLowerCase()))
    )
      throw new Error(`候选运行库出现档案之外的直接依赖：${item.path}`);
  }
}

// 只有锁定的原始工件、许可正文和运行库同时匹配才登记为有来源的材料。
export async function verifyThirdPartyMaterials(
  repository,
  materialsDirectory,
  binaries,
  profileName,
) {
  if (materialsDirectory) materialsDirectory = path.resolve(materialsDirectory);
  const { base, lockPath } = await materialsPaths(repository, profileName);
  const lock = JSON.parse(await fs.readFile(lockPath, 'utf8'));
  if (lock.sourceClosure.ffmpegStaticDependencies === 'complete' && !lock.runtimePolicy)
    throw new Error('完整源码档案必须同时限定实际构建配置、许可和运行库导入边界');
  const files = [];
  const textFiles = [];
  const gaps = [];
  for (const text of lock.texts) {
    if (text.external && !materialsDirectory) {
      gaps.push(`第三方许可原文未提供：${text.path}`);
      continue;
    }
    const textBase = text.external ? materialsDirectory : base;
    const filename = path.resolve(textBase, text.external ? text.externalPath : text.path);
    if (!filename.startsWith(`${textBase}${path.sep}`)) throw new Error('许可清单路径越界');
    await rejectMaterialLinks(filename);
    const stat = await fs.lstat(filename);
    if (stat.isSymbolicLink() || !stat.isFile()) throw new Error('许可正文不是普通文件');
    if ((await sha256(filename)) !== text.sha256)
      throw new Error(`第三方许可正文指纹不匹配：${text.path}`);
    files.push({ source: filename, target: `licenses/${text.path}` });
    textFiles.push({ source: filename, target: `licenses/${text.path}` });
  }
  for (const binary of lock.binaries) {
    const filename = binaries.get(`${binary.group}/${binary.file}`);
    if (!filename || (await sha256(filename)) !== binary.sha256)
      throw new Error(`第三方运行库与固定发行材料不匹配：${binary.group}/${binary.file}`);
  }
  const artifacts = [];
  for (const artifact of lock.artifacts) {
    if (!/^[\w.-]+$/.test(artifact.file) || ['.', '..'].includes(artifact.file))
      throw new Error('第三方工件名称不能包含目录');
    if (!materialsDirectory) {
      gaps.push(`第三方原始工件未提供：${artifact.file}`);
      continue;
    }
    const filename = path.join(materialsDirectory, artifact.file);
    try {
      await rejectMaterialLinks(filename);
      const stat = await fs.lstat(filename);
      if (stat.isSymbolicLink() || !stat.isFile())
        throw new Error(`第三方工件不是普通文件：${artifact.file}`);
      if (stat.size !== artifact.bytes || (await sha256(filename)) !== artifact.sha256)
        throw new Error(`第三方工件指纹不匹配：${artifact.file}`);
      artifacts.push(artifact);
      if (
        artifact.packagePath &&
        (!/^licenses\/[\w./-]+$/.test(artifact.packagePath) ||
          artifact.packagePath.split('/').some((part) => part === '..'))
      )
        throw new Error('第三方工件打包路径必须位于 licenses 下');
      if (artifact.packagePath || ['ffmpeg-source', 'ffmpeg-build-recipes'].includes(artifact.id))
        files.push({
          source: filename,
          target: artifact.packagePath || `licenses/ffmpeg/sources/${artifact.file}`,
        });
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
      gaps.push(`第三方原始工件缺失：${artifact.file}`);
    }
  }
  if (lock.sourceClosure.ffmpegStaticDependencies !== 'complete')
    gaps.push(
      'FFmpeg 静态集成第三方组件的完整源码及声明未闭合；主源码和构建配方不能代替对应源码闭包',
    );
  files.push({
    source: lockPath,
    target: 'licenses/materials-lock.json',
  });
  files.push({ source: path.join(base, 'README.md'), target: 'licenses/README.md' });
  files.push({ source: path.join(base, 'NOTICE.md'), target: 'licenses/NOTICE.md' });
  return {
    files,
    textFiles,
    gaps,
    artifacts,
    profileName: profileName || 'btbn-20261022',
    sourceClosure: lock.sourceClosure,
    vcRuntime: lock.vcRuntime,
    runtimePolicy: lock.runtimePolicy,
  };
}
