import { execFileSync } from 'node:child_process';
import { promises as fs } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  cLayoutProbe,
  parseAbiTypes,
  parseLayout,
  parseStatusCodes,
  renderRustTypes,
} from './abi-model.mjs';
import { renderDefaults } from './abi-defaults.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const preamble = '// 由scripts/sdk/generate-rust-abi.mjs生成；与公开C头文件同源，勿手改。\n';

function format(text) {
  return execFileSync('rustfmt', ['--edition', '2021', '--emit', 'stdout'], {
    input: text,
    encoding: 'utf8',
  });
}

async function measure(model) {
  if (process.platform !== 'win32' || process.arch !== 'x64')
    throw new Error('布局基线仅允许Windows x64编译器生成');
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'am-sdk-layout-'));
  const source = path.join(directory, 'probe.c');
  const executable = path.join(directory, 'probe.exe');
  try {
    await fs.writeFile(source, cLayoutProbe(model));
    execFileSync('gcc', [
      '-std=c11',
      '-Wall',
      '-Wextra',
      '-Werror',
      '-I',
      path.join(root, 'sdk/include'),
      source,
      '-o',
      executable,
    ]);
    return parseLayout(execFileSync(executable, [], { encoding: 'utf8' }), model);
  } finally {
    // 探针目录只含本次两个普通文件，逐个删除，避免递归越过目录链接。
    for (const file of [source, executable])
      await fs.unlink(file).catch((error) => {
        if (error.code !== 'ENOENT') throw error;
      });
    await fs.rmdir(directory);
  }
}

export async function generateRustAbi({ check = false } = {}) {
  const [main, types, configText, defaultsText] = await Promise.all([
    fs.readFile(path.join(root, 'docs/design/dual-dll-abi.md'), 'utf8'),
    fs.readFile(path.join(root, 'docs/design/dual-dll-abi-types.md'), 'utf8'),
    fs.readFile(path.join(root, 'sdk/abi/constants.json'), 'utf8'),
    fs.readFile(path.join(root, 'sdk/abi/defaults.json'), 'utf8'),
  ]);
  const config = JSON.parse(configText);
  const model = parseAbiTypes(main, types);
  const measured = await measure(model);
  const files = renderRustTypes(model);
  files['defaults.rs'] = renderDefaults(JSON.parse(defaultsText), model);
  const definedEnums = new Set(
    Object.values(model.enums)
      .flat()
      .map((v) => v.name),
  );
  files.constants = Object.entries(config.constants)
    .filter(([name]) => !definedEnums.has(name))
    .map(([name, value]) => `pub const ${name}: u32 = ${value};`)
    .join('\n');
  for (const [prefix, names] of [
    ['AMC_CAP_', config.cameraCapabilities],
    ['AMV_CAP_', config.visionCapabilities],
  ]) {
    files.constants +=
      '\n' + names.map((name, i) => `pub const ${prefix}${name}: u64 = 1 << ${i};`).join('\n');
  }
  files['constants.rs'] = files.constants;
  delete files.constants;
  const statuses = parseStatusCodes(types);
  files['status.rs'] = statuses
    .map((status) => `pub const ${status.constant}: am_status_t = ${status.value};`)
    .join('\n');
  files['status.rs'] +=
    "\npub fn status_name(status: am_status_t) -> Option<&'static core::ffi::CStr> {\nmatch status {\n" +
    statuses.map((status) => `${status.value} => Some(c"${status.name}"),`).join('\n') +
    '\n_ => None,\n}\n}\n';
  const modules = Object.keys(files).map((name) => name.replace('.rs', ''));
  files['lib.rs'] =
    `//! 两个导出库共用的C ABI值类型；不含业务、分配器或第三方运行库。\n#![allow(non_camel_case_types)]\nuse core::ffi::{c_char, c_void};\n${modules.map((name) => `mod ${name};\n` + (name.startsWith('fields_') ? '' : `pub use ${name}::*;`)).join('\n')}\n#[cfg(all(test, target_os = "windows", target_arch = "x86_64"))]\nmod layout_tests;\n`;
  const tests = Object.entries(measured).map(([name, layout]) => {
    const offsets = Object.entries(layout.offsets)
      .map(
        ([field, offset]) =>
          `assert_eq!(core::mem::offset_of!(${name}, ${field}), ${offset}, "${name}.${field}");`,
      )
      .join('\n');
    return `#[test]\nfn ${name}_matches_c() {\nassert_eq!(core::mem::size_of::<${name}>(), ${layout.size});\nassert_eq!(core::mem::align_of::<${name}>(), ${layout.align});\n${offsets}\n}`;
  });
  for (let i = 0; i < tests.length; i += 10)
    files[`layout_${i / 10}.rs`] = tests.slice(i, i + 10).join('\n');
  files['layout_tests.rs'] = Array.from(
    { length: Math.ceil(tests.length / 10) },
    (_, i) => `#[path = "layout_${i}.rs"]\nmod group_${i};`,
  ).join('\n');
  const directory = path.join(root, 'crates/sdk-abi/src');
  if (!check) await fs.mkdir(directory, { recursive: true });
  for (const [name, content] of Object.entries(files)) {
    const imports = ['lib.rs', 'constants.rs', 'layout_tests.rs'].includes(name)
      ? ''
      : name.startsWith('layout_')
        ? 'use crate::*;\n'
        : 'use super::*;\n';
    const generated = format(preamble + imports + content);
    const target = path.join(directory, name);
    if (check) {
      if ((await fs.readFile(target, 'utf8')) !== generated)
        throw new Error(`Rust ABI漂移：${name}`);
    } else await fs.writeFile(target, generated);
  }
  const baseline = Object.fromEntries(
    Object.entries(measured).map(([name, l]) => [name, { size: l.size, align: l.align }]),
  );
  const layoutFile = path.join(root, 'sdk/abi/layout-win-x64.json');
  const json =
    JSON.stringify({ target: 'windows-x64-default-packing', structures: baseline }, null, 2) + '\n';
  if (check) {
    if ((await fs.readFile(layoutFile, 'utf8')) !== json) throw new Error('C布局基线漂移');
  } else await fs.writeFile(layoutFile, json);
  return {
    structures: model.structures.length,
    fields: Object.values(measured).reduce((sum, l) => sum + Object.keys(l.offsets).length, 0),
    files: Object.keys(files).length,
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  generateRustAbi({ check: process.argv.includes('--check') }).then(
    (result) => console.log(JSON.stringify(result)),
    (error) => {
      console.error(error.message);
      process.exitCode = 1;
    },
  );
}
