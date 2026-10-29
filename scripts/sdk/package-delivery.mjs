import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createReadStream, promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertRuntimePolicy, verifyThirdPartyMaterials } from './third-party-materials.mjs';
import { verifyBuildRecord } from './build-record.mjs';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const ffmpegNames = [
  'avcodec-63.dll',
  'avformat-63.dll',
  'avutil-61.dll',
  'swresample-7.dll',
  'swscale-10.dll',
  // ⑤ GPL 编码构建闭包（2026-10-29 决策）：
  'libx264-165.dll',
];
const usage = `本地 Windows x64 SDK 交付包（不上传、不发布）
node scripts/sdk/package-delivery.mjs --output "${path.join(repo, '.local/releases/new-package')}" \\
  --dll-dir "${path.join(repo, '.local/build/current/debug')}" \\
  --ffmpeg-dir "${path.join(repo, '.local/runtimes/ffmpeg')}" \\
  --ort-dir "${path.join(repo, '.local/runtimes/ort')}" \\
  [--materials-dir "${path.join(repo, '.local/runtimes/materials')}"] [--python python]
  [--materials-profile <仓库受控材料档案名>]
  [--build-record <已成功Release构建的记录JSON>]
  [--require-distribution-ready]
输出目录必须在本仓 .local/releases、.local/evidence 或 .local/tests 下新建，不允许目录链接。
许可证材料不齐时仍可生成本地核验包，但清单明确标记 incomplete，不能当作正式分发合规证明。`;

function inside(root, candidate) {
  const rel = path.relative(root, candidate);
  return rel === '' || (!rel.startsWith(`..${path.sep}`) && rel !== '..' && !path.isAbsolute(rel));
}

async function rejectLinks(value) {
  let current = path.resolve(value);
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

function git(...args) {
  return execFileSync('git', ['-C', repo, ...args], {
    encoding: 'utf8',
  }).trim();
}

async function digest(filename) {
  const hash = createHash('sha256');
  for await (const data of createReadStream(filename)) hash.update(data);
  return hash.digest('hex');
}

export async function inspectPe(filename) {
  const handle = await fs.open(filename, 'r');
  try {
    const length = (await handle.stat()).size;
    async function read(offset, size) {
      if (offset < 0 || offset + size > length) throw new Error(`PE 数据越界：${filename}`);
      const bytes = Buffer.alloc(size);
      if ((await handle.read(bytes, 0, size, offset)).bytesRead !== size)
        throw new Error(`PE 文件读取不完整：${filename}`);
      return bytes;
    }
    const dos = await read(0, 64);
    if (dos.toString('ascii', 0, 2) !== 'MZ') throw new Error(`不是 PE 文件：${filename}`);
    const peOffset = dos.readUInt32LE(60);
    const header = await read(peOffset, 24);
    if (header.readUInt32LE(0) !== 0x4550 || header.readUInt16LE(4) !== 0x8664)
      throw new Error(`不是 Windows x64 PE：${filename}`);
    const count = header.readUInt16LE(6);
    const optionalSize = header.readUInt16LE(20);
    if (count > 96 || optionalSize < 128 || optionalSize > 4096)
      throw new Error(`PE 头大小无效：${filename}`);
    const optional = await read(peOffset + 24, optionalSize);
    if (optional.readUInt16LE(0) !== 0x20b) throw new Error(`不是 PE32+：${filename}`);
    const sections = await read(peOffset + 24 + optionalSize, count * 40);
    function offset(rva) {
      for (let i = 0; i < count; i++) {
        const start = i * 40;
        const address = sections.readUInt32LE(start + 12);
        const rawBytes = sections.readUInt32LE(start + 16);
        if (rva >= address && rva - address < rawBytes)
          return sections.readUInt32LE(start + 20) + rva - address;
      }
      if (rva < optional.readUInt32LE(60)) return rva;
      throw new Error(`PE RVA 无原始文件映射：${filename}`);
    }
    const importsRva = optional.readUInt32LE(120);
    const imports = [];
    if (importsRva) {
      let row = offset(importsRva);
      for (let i = 0; i < 512; i++, row += 20) {
        const descriptor = await read(row, 20);
        if (descriptor.every((byte) => byte === 0)) break;
        const nameOffset = offset(descriptor.readUInt32LE(12));
        const name = await read(nameOffset, Math.min(512, length - nameOffset));
        const end = name.indexOf(0);
        if (end < 0 || !/^[\w.-]+\.dll$/i.test(name.toString('ascii', 0, end)))
          throw new Error(`PE 导入名称无效：${filename}`);
        imports.push(name.toString('ascii', 0, end));
        if (i === 511) throw new Error(`PE 导入表超限：${filename}`);
      }
    }
    return {
      architecture: 'windows-x64',
      directImports: [...new Set(imports)],
    };
  } finally {
    await handle.close();
  }
}

async function directory(value, name) {
  if (!value || !path.isAbsolute(value)) throw new Error(`--${name} 必须是绝对目录`);
  await rejectLinks(value);
  const actual = await fs.realpath(value);
  if (!(await fs.stat(actual)).isDirectory()) throw new Error(`不是目录：${actual}`);
  return actual;
}

async function filesUnder(root) {
  const result = [];
  async function visit(relative) {
    const current = path.join(root, relative);
    await rejectLinks(current);
    for (const entry of await fs.readdir(current, { withFileTypes: true })) {
      const child = path.join(relative, entry.name);
      if (entry.isSymbolicLink()) throw new Error(`拒绝目录内链接：${path.join(root, child)}`);
      if (entry.isDirectory()) await visit(child);
      else if (entry.isFile()) result.push(child);
      else throw new Error(`不支持的文件类型：${path.join(root, child)}`);
    }
  }
  await visit('');
  return result.sort();
}

function nativeMetadata(ffmpeg, ort, python) {
  const code = `import ctypes as c,json,os,sys
ffmpeg,ort=sys.argv[1:3]
guards=[os.add_dll_directory(ffmpeg),os.add_dll_directory(ort)]
rows=[]
for stem,name in [('avutil','avutil-61.dll'),('avcodec','avcodec-63.dll'),('avformat','avformat-63.dll'),('swresample','swresample-7.dll'),('swscale','swscale-10.dll')]:
 lib=c.CDLL(os.path.join(ffmpeg,name))
 version=getattr(lib,stem+'_version');version.restype=c.c_uint
 config=getattr(lib,stem+'_configuration');config.restype=c.c_char_p
 license=getattr(lib,stem+'_license');license.restype=c.c_char_p
 raw=version()
 rows.append(dict(file=name,version=f'{raw>>16}.{(raw>>8)&255}.{raw&255}',configuration=config().decode(),license=license().decode()))
class Base(c.Structure):
 _fields_=[('get_api',c.c_void_p),('get_version',c.CFUNCTYPE(c.c_char_p))]
lib=c.CDLL(os.path.join(ort,'onnxruntime.dll'))
get=lib.OrtGetApiBase;get.restype=c.POINTER(Base)
print(json.dumps(dict(ffmpeg=rows,onnxruntime=dict(version=get().contents.get_version().decode(),configuration='CPU runtime, explicit directory; provider execution is not tested by packaging'))))`;
  return JSON.parse(
    execFileSync(python, ['-c', code, ffmpeg, ort], {
      encoding: 'utf8',
      timeout: 20_000,
      maxBuffer: 1024 * 1024,
      windowsHide: true,
    }),
  );
}

export async function packageDelivery(options) {
  if (process.platform !== 'win32' || process.arch !== 'x64')
    throw new Error('本打包器首版只在 Windows x64 运行；其他平台未验收');
  if (!options.output || !path.isAbsolute(options.output))
    throw new Error('--output 必须是绝对路径');
  const output = path.resolve(options.output);
  const root = await fs.realpath(repo);
  if (
    !['releases', 'evidence', 'tests'].some((group) => {
      const parent = path.join(root, '.local', group);
      return output !== parent && inside(parent, output);
    })
  )
    throw new Error('输出必须在 SDK .local/releases、.local/evidence 或 .local/tests 分类下');
  await rejectLinks(output);
  try {
    if ((await fs.readdir(output)).length) throw new Error('输出目录非空，拒绝覆盖');
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  const dll = await directory(options.dllDir, 'dll-dir');
  const ffmpeg = await directory(options.ffmpegDir, 'ffmpeg-dir');
  const ort = await directory(options.ortDir, 'ort-dir');
  const ortLicenses = options.ortLicensesDir
    ? await directory(options.ortLicensesDir, 'ort-licenses-dir')
    : null;
  const materials = options.materialsDir
    ? await directory(options.materialsDir, 'materials-dir')
    : null;
  if (
    [dll, ffmpeg, ort, ortLicenses, materials]
      .filter(Boolean)
      .some((source) => inside(output, source) || inside(source, output))
  )
    throw new Error('输出目录与输入目录不能互相包含');
  const binaries = [
    ['sdk-dll', dll, 'am_camera.dll', 'bin/win-x64'],
    ['sdk-dll', dll, 'am_vision.dll', 'bin/win-x64'],
    ...ffmpegNames.map((name) => ['ffmpeg', ffmpeg, name, 'runtime/ffmpeg']),
    ['onnxruntime-cpu', ort, 'onnxruntime.dll', 'runtime/onnxruntime-cpu'],
  ];
  const binaryEvidence = [];
  for (const [group, base, name, subdirectory] of binaries) {
    await rejectLinks(path.join(base, name));
    binaryEvidence.push({
      path: `${subdirectory}/${name}`,
      group,
      ...(await inspectPe(path.join(base, name))),
    });
  }
  let buildRecord = null;
  if (options.buildRecord) {
    await rejectLinks(path.resolve(options.buildRecord));
    buildRecord = await verifyBuildRecord(options.buildRecord, dll);
    git('cat-file', '-e', `${buildRecord.sourceCommit}^{commit}`);
  }
  const metadata = nativeMetadata(ffmpeg, ort, options.python || 'python');
  const thirdParty = await verifyThirdPartyMaterials(
    root,
    materials,
    new Map(binaries.map(([group, base, name]) => [`${group}/${name}`, path.join(base, name)])),
    options.materialsProfile,
  );
  assertRuntimePolicy(thirdParty.runtimePolicy, metadata.ffmpeg, binaryEvidence);
  const lockedLicenseText = (
    await Promise.all(
      thirdParty.textFiles
        .filter((file) => file.target.startsWith('licenses/ffmpeg/'))
        .map((file) => fs.readFile(file.source, 'utf8')),
    )
  ).join('\n');
  const licenseGaps = [...thirdParty.gaps];
  if (metadata.ffmpeg.some((item) => item.configuration.includes('--enable-nonfree')))
    licenseGaps.push('FFmpeg --enable-nonfree build requires explicit distribution review');
  for (const [runtimeLicense, expression, description] of [
    [/^GPL version 3/, /GNU GENERAL PUBLIC LICENSE[\s\S]{0,60}Version 3/, 'GPLv3'],
    [/^LGPL version 3/, /GNU LESSER GENERAL PUBLIC LICENSE[\s\S]{0,60}Version 3/, 'LGPLv3'],
    [/^LGPL version 2\.1/, /GNU LESSER GENERAL PUBLIC LICENSE[\s\S]{0,60}Version 2\.1/, 'LGPLv2.1'],
  ]) {
    if (
      metadata.ffmpeg.some((item) => runtimeLicense.test(item.license)) &&
      !expression.test(lockedLicenseText)
    )
      licenseGaps.push(`FFmpeg actual ${description} runtime lacks matching locked license text`);
  }
  if (options.requireDistributionReady && licenseGaps.length)
    throw new Error(`正式分发材料未齐，拒绝生成：${licenseGaps.join('；')}`);
  const commit = git('rev-parse', 'HEAD');
  const status = git('status', '--porcelain=v1', '--untracked-files=all');
  const changes = git('diff', 'HEAD', '--name-only', '-z').split('\0').filter(Boolean);
  const untracked = git('ls-files', '--others', '--exclude-standard', '-z')
    .split('\0')
    .filter(Boolean);
  const tracked = new Set(git('ls-files', '-z').split('\0').filter(Boolean));
  const dirtyFiles = new Set([...changes, ...untracked]);
  const rows = [];
  await fs.mkdir(output, { recursive: true });
  async function copy(source, relative, group) {
    await rejectLinks(source);
    const stat = await fs.stat(source);
    if (!stat.isFile()) throw new Error(`非普通文件：${source}`);
    const target = path.resolve(output, relative);
    if (!inside(output, target) || target === output) throw new Error(`输出路径越界：${relative}`);
    await rejectLinks(target);
    await fs.mkdir(path.dirname(target), { recursive: true });
    await fs.copyFile(source, target, 1);
    const sourceHash = await digest(source);
    const targetHash = await digest(target);
    if (sourceHash !== targetHash) throw new Error(`复制期间源文件改变：${source}`);
    const repositoryFile = inside(root, source)
      ? path.relative(root, source).split(path.sep).join('/')
      : null;
    rows.push({
      path: path.relative(output, target).split(path.sep).join('/'),
      bytes: (await fs.stat(target)).size,
      sha256: targetHash,
      group,
      source: {
        path: source,
        repositoryCommit: repositoryFile && tracked.has(repositoryFile) ? commit : null,
        tracked: repositoryFile ? tracked.has(repositoryFile) : false,
        dirty: repositoryFile ? dirtyFiles.has(repositoryFile) : null,
        provenance:
          repositoryFile && tracked.has(repositoryFile)
            ? 'working-tree-copy'
            : 'caller-supplied-binary-or-license',
      },
    });
  }
  for (const [group, base, name, subdirectory] of binaries)
    await copy(path.join(base, name), `${subdirectory}/${name}`, group);
  for (const material of thirdParty.files)
    await copy(material.source, material.target, 'verified-third-party-material');
  for (const name of ['am_camera.dll.lib', 'am_vision.dll.lib'])
    await copy(path.join(dll, name), `lib/win-x64/${name}`, 'sdk-import-library');
  for (const subdirectory of ['include', 'bindings', 'examples', 'abi']) {
    const sourceRoot = path.join(root, 'sdk', subdirectory);
    for (const relative of await filesUnder(sourceRoot)) {
      if (
        relative.split(path.sep).some((part) => part === '__pycache__') ||
        /\.(pyc|exe|dll|onnx|pt|pth|safetensors|bin|db|sqlite|sqlite3)$/i.test(relative)
      )
        continue;
      await copy(path.join(sourceRoot, relative), `sdk/${subdirectory}/${relative}`, 'sdk');
    }
  }
  await copy(path.join(root, 'sdk/README.md'), 'sdk/README.md', 'source-guidance');
  await copy(path.join(root, 'sdk/INTEGRATION.md'), 'SDK-接入说明.md', 'integration-guidance');
  if (buildRecord)
    await copy(path.join(root, 'sdk/RELEASE-NOTES.md'), 'RELEASE-NOTES.md', 'release-guidance');
  if (buildRecord) await copy(options.buildRecord, 'build-record.json', 'build-record');
  await copy(path.join(root, 'sdk/runtime/README.md'), 'runtime/README.md', 'runtime-guidance');
  await copy(
    path.join(root, 'crates/vision/resources/profiles.json'),
    'sdk/profiles.json',
    'model-profiles',
  );
  for (const name of [
    'dual-dll-sdk.md',
    'dual-dll-abi.md',
    'dual-dll-abi-types.md',
    'dual-sdk-abi-freeze-v1.md',
    'dual-sdk-shared-frames.md',
  ])
    await copy(path.join(root, 'docs/design', name), `docs/design/${name}`, 'design-contract');
  for (const name of buildRecord
    ? []
    : [
        'verification-dual-sdk-implementation-20261025.md',
        'verification-dual-sdk-remote-20261026.md',
        'verification-dual-sdk-closeout-20261026.md',
      ])
    await copy(
      path.join(root, 'docs/verification', name),
      `docs/verification/${name}`,
      'verification-record',
    );
  const sourceGuide =
    '# 独立源码交付\n\n完整源码另由 export-source.mjs 导出，按 sdk-source-manifest.json 的 sourceCommit 与本包核对。此目录不是可编译源码闭包；本包的使用说明位于 ../sdk/README.md，模型档案在 ../sdk/profiles.json。C/C++ 的 Windows x64 导入库在 ../lib/win-x64，运行DLL在 ../bin/win-x64。构建与运行验证分别见 ../docs/verification/。\n';
  await fs.mkdir(path.join(output, 'source'), { recursive: true });
  await fs.writeFile(path.join(output, 'source/README.md'), sourceGuide, {
    flag: 'wx',
  });
  rows.push({
    path: 'source/README.md',
    bytes: Buffer.byteLength(sourceGuide),
    sha256: createHash('sha256').update(sourceGuide).digest('hex'),
    group: 'generated-guidance',
    source: {
      path: 'scripts/sdk/package-delivery.mjs',
      repositoryCommit: commit,
      dirty: dirtyFiles.has('scripts/sdk/package-delivery.mjs'),
      provenance: 'generated-from-working-tree-script',
    },
  });
  const ffmpegLicenses = (await filesUnder(ffmpeg)).filter(
    (file) =>
      /(?:^|[/\\])(?:license|copying|notice|readme|source|build)/i.test(file) &&
      (/\.(txt|md|html|json)$/i.test(file) || !path.extname(file)),
  );
  for (const relative of ffmpegLicenses)
    await copy(path.join(ffmpeg, relative), `licenses/ffmpeg/${relative}`, 'ffmpeg-license');
  const ortLicenseFiles = ortLicenses ? await filesUnder(ortLicenses) : [];
  for (const relative of ortLicenseFiles) {
    if (
      !/\.(txt|md|html|json)$/i.test(relative) &&
      !/^(license|copying|notice|third.?party.?notices)$/i.test(path.basename(relative))
    )
      throw new Error(`ORT 许可证目录只允许说明文档：${relative}`);
    if ((await fs.stat(path.join(ortLicenses, relative))).size > 8 * 1024 * 1024)
      throw new Error(`ORT 单份说明材料超过 8 MiB：${relative}`);
    await copy(
      path.join(ortLicenses, relative),
      `licenses/onnxruntime/${relative}`,
      'onnxruntime-license',
    );
  }
  const gaps = licenseGaps;
  // 旧参数仅作为额外材料复制；不能替代已与实际 DLL 锁定匹配的原始 wheel 证明。
  const manifest = {
    schemaVersion: 1,
    purpose: buildRecord
      ? 'prepared-release-candidate-no-publication'
      : 'local-verification-package-only',
    createdAt: new Date().toISOString(),
    source: { repository: root, commit, dirty: status !== '', status },
    platform: {
      build: 'windows-x64',
      verification: 'metadata-and-file-integrity-only',
      crossPlatformValidated: false,
    },
    sdkBuild: {
      configuration: buildRecord?.configuration || 'not inferred from input directory',
      buildSourceCommit: buildRecord?.sourceCommit || null,
      record: buildRecord ? 'build-record.json' : null,
      binaryBuildCommitVerified: false,
      note: 'DLL hash identifies bytes; this script does not assert those bytes were built from source.commit or execute SDK tests.',
    },
    runtimes: metadata,
    binaryEvidence,
    windowsPrerequisites: {
      vcRuntime: {
        requirement:
          'Microsoft Visual C++ 2015–2022 Redistributable x64 (14.x ABI), compatible with the build toolset',
        packaged: false,
        evidence: binaryEvidence
          .filter((binary) =>
            binary.directImports.some((name) => /^(?:vcruntime|msvcp)140/i.test(name)),
          )
          .map((binary) => ({
            path: binary.path,
            imports: binary.directImports.filter((name) => /^(?:vcruntime|msvcp)140/i.test(name)),
          })),
        cleanMachineVerified: false,
        note: 'PE direct imports are observed facts; successful native metadata reads only show this development process can load the dependencies. No redistributable installer/version or clean-machine sufficiency is certified.',
      },
      diagnosticBoundary: {
        sdkDllLoad:
          'Missing SDK transitive VC runtime can fail the caller native loader before any SDK init/error API is callable; Windows 126/193 or language loader exceptions need caller handling.',
        ortInitialization:
          'With SDK loaded, missing ORT file returns vision.runtimeMissing; load/API/dependency failures return vision.runtimeLoadFailed, without a guaranteed missing transitive DLL name.',
        ffmpegLoading:
          'Camera init does not eagerly require FFmpeg. Decoder and RTSP failures retain the frozen status and expose component/load/symbol details through the existing error interface; the exact missing transitive dependency is not always identifiable.',
      },
    },
    runtimeDirectories: {
      ffmpeg: 'runtime/ffmpeg',
      onnxruntimeCpu: 'runtime/onnxruntime-cpu',
      acceleratedProviders: 'separate-package-not-included',
    },
    licensing: {
      status: gaps.length ? 'incomplete' : 'materials-present-review-required',
      distributionApproved: false,
      gaps,
      verifiedArtifacts: thirdParty.artifacts,
      profile: thirdParty.profileName,
      sourceClosure: thirdParty.sourceClosure,
      vcRuntime: thirdParty.vcRuntime,
      note: 'Locked texts and binaries are SHA-256 matched to fixed upstream artifacts. FFmpeg main source and recipes are bundled when supplied; static dependency closure remains explicitly gated. No external release approval is inferred.',
    },
    sourceSdk: {
      included: false,
      guidance: 'source/README.md',
      exportCommand:
        'node scripts/sdk/export-source.mjs --output <new-absolute-directory-outside-repository>',
    },
    fileInventoryScope:
      'Copied files and generated README; the manifest itself is excluded to avoid a recursive fingerprint. Its SHA-256 is returned by the command.',
    files: rows,
  };
  const title = buildRecord ? 'Windows x64 Release 候选包' : '本地核验包';
  const readme = `# AM-RT SDK ${title}\n\n来源提交：${commit}；工作树${status ? '有未提交变化，见 runtime-manifest.json' : '干净'}。\n\n先读 SDK-接入说明.md。本包包含双 DLL、C 头文件、语言封装、示例和 ABI 清单。完整独立源码另包交付，本目录不冒充源码闭包。\n\n调用方给 camera init 传本包 runtime/ffmpeg 绝对目录，给 vision init 传 runtime/onnxruntime-cpu 绝对目录；SDK DLL 放 bin/win-x64。两运行库目录不能合并，不覆盖系统 PATH。OpenVINO/CUDA 依赖另包。\n\n许可证状态：${manifest.licensing.status}；缺项：${gaps.length ? gaps.join('；') : '无已登记的材料缺口'}。本包未批准对外发布，本打包命令不执行功能验收。\n\n逐文件 SHA/大小/来源见 runtime-manifest.json。${buildRecord ? 'Release 构建记录见 build-record.json，有限验证范围见 RELEASE-NOTES.md；内部测试日志、环境与原始证据独立归档，不随包提供。' : 'DLL 与来源提交的构建关系没有自动证明；不能单凭打包成功宣称 SDK-G3 通过。'}\n`;

  await fs.writeFile(path.join(output, 'README.md'), readme, { flag: 'wx' });
  rows.push({
    path: 'README.md',
    bytes: Buffer.byteLength(readme),
    sha256: createHash('sha256').update(readme).digest('hex'),
    group: 'generated-guidance',
    source: {
      path: 'scripts/sdk/package-delivery.mjs',
      repositoryCommit: commit,
      dirty: dirtyFiles.has('scripts/sdk/package-delivery.mjs'),
      provenance: 'generated-from-working-tree-script',
    },
  });
  await fs.writeFile(
    path.join(output, 'runtime-manifest.json'),
    `${JSON.stringify(manifest, null, 2)}\n`,
    { flag: 'wx' },
  );
  return {
    output,
    copiedFiles: rows.length,
    licensing: manifest.licensing.status,
    licenseGaps: gaps,
    manifestSha256: await digest(path.join(output, 'runtime-manifest.json')),
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const names = new Map([
    ['--output', 'output'],
    ['--dll-dir', 'dllDir'],
    ['--ffmpeg-dir', 'ffmpegDir'],
    ['--ort-dir', 'ortDir'],
    ['--ort-licenses-dir', 'ortLicensesDir'],
    ['--materials-dir', 'materialsDir'],
    ['--materials-profile', 'materialsProfile'],
    ['--build-record', 'buildRecord'],
    ['--python', 'python'],
  ]);
  try {
    const options = {};
    for (let i = 2; i < process.argv.length; i++) {
      if (process.argv[i] === '--help') {
        console.log(usage);
        process.exit(0);
      }
      if (process.argv[i] === '--require-distribution-ready') {
        if (options.requireDistributionReady)
          throw new Error('重复参数：--require-distribution-ready');
        options.requireDistributionReady = true;
        continue;
      }
      const key = names.get(process.argv[i]);
      if (!key || !process.argv[i + 1] || process.argv[i + 1].startsWith('--'))
        throw new Error(`参数无效：${process.argv[i]}`);
      if (options[key]) throw new Error(`重复参数：${process.argv[i]}`);
      options[key] = process.argv[++i];
    }
    console.log(JSON.stringify(await packageDelivery(options), null, 2));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
