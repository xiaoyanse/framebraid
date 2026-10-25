import { promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseStatusCodes } from './abi-model.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const strip = (text) => text.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/[^\n]*/g, '');
const blocks = (text) => [...text.matchAll(/```c\r?\n([\s\S]*?)```/g)].map((m) => m[1]).join('\n');

function wrap(guard, content) {
  return `/* C ABI公开头文件；由scripts/sdk/generate-abi.mjs生成，勿手工修改。 */
#ifndef ${guard}
#define ${guard}
${content}
#endif
`.replace(/[ \t]+$/gm, '');
}

export function generateHeaders(mainDocument, typesDocument, constants) {
  const main = strip(blocks(mainDocument));
  let types = blocks(typesDocument);
  const view = main.match(
    /typedef struct am_pixel_view_s\s*\{[\s\S]*?}\s*am_pixel_view_t\s*;/,
  )?.[0];
  if (!view) throw new Error('缺少公共像素视图结构');
  types += `\n${view}\n`;
  const cleanTypes = strip(types);
  const known = new Set([...cleanTypes.matchAll(/}\s*(\w+)\s*;/g)].map((m) => m[1]));
  const enums = [...main.matchAll(/typedef enum\s+\w+\s*\{[\s\S]*?}\s*(am\w+_e)\s*;/g)]
    .filter((m) => !known.has(m[1]))
    .map((m) => m[0]);
  const functions = [...main.matchAll(/AM_API\s+[^;]+;/g)].map((m) => m[0]);
  const functionNames = functions.map((text) => text.match(/\b(am[cv]_\w+)\s*\(/)?.[1]);
  if (functionNames.some((name) => !name) || new Set(functionNames).size !== functions.length) {
    throw new Error('导出函数缺少名称或重复');
  }
  const callbacks = [...main.matchAll(/typedef\s+\w+\s+\(\*\w+\)\([\s\S]*?\);/g)].map((m) => m[0]);
  const forwards = [...cleanTypes.matchAll(/typedef struct (\w+)\s*\{[\s\S]*?}\s*(\w+)\s*;/g)].map(
    (m) => `typedef struct ${m[1]} ${m[2]};`,
  );
  const used = [
    ...new Set(
      [
        ...(cleanTypes + functions.join('\n') + callbacks.join('\n')).matchAll(
          /\b(am[cv]_\w+_t)\b/g,
        ),
      ].map((m) => m[1]),
    ),
  ];
  const handles = used
    .filter((name) => !known.has(name))
    .map((name) => `typedef struct ${name.slice(0, -2)}_s ${name};`);
  const enumNames = new Set(
    [...(cleanTypes + enums.join('\n')).matchAll(/\b(AM_[A-Z0-9_]+)\b/g)].map((m) => m[1]),
  );
  const definitions = Object.entries(constants.constants)
    .filter(([name]) => !enumNames.has(name))
    .map(([name, value]) => `#define ${name} UINT32_C(${value})`);
  definitions.push(
    ...parseStatusCodes(typesDocument).map(
      (status) => `#define ${status.constant} INT32_C(${status.value})`,
    ),
  );
  for (const [prefix, names] of [
    ['AMC_CAP_', constants.cameraCapabilities],
    ['AMV_CAP_', constants.visionCapabilities],
  ]) {
    names.forEach((name, i) => definitions.push(`#define ${prefix}${name} (UINT64_C(1) << ${i})`));
  }
  const common = wrap(
    'AM_SDK_TYPES_H',
    `#include <stdint.h>
#include <stddef.h>
#include <limits.h>
#ifndef AM_API
#if defined(_WIN32) && defined(AM_SDK_BUILD)
#define AM_API __declspec(dllexport)
#else
#define AM_API
#endif
#endif
#ifdef __cplusplus
extern "C" {
#endif
${forwards.join('\n')}
${handles.join('\n')}
${enums.join('\n')}
${definitions.join('\n')}
${types}
${callbacks.join('\n')}
#ifdef __cplusplus
}
#endif`,
  );
  const result = { 'am_sdk_types.h': common };
  for (const [prefix, filename] of [
    ['amc_', 'am_camera.h'],
    ['amv_', 'am_vision.h'],
  ]) {
    result[filename] = wrap(
      filename.toUpperCase().replaceAll('.', '_'),
      `#include "am_sdk_types.h"
#ifdef __cplusplus
extern "C" {
#endif
${functions.filter((_, i) => functionNames[i].startsWith(prefix)).join('\n')}
#ifdef __cplusplus
}
#endif`,
    );
  }
  return { headers: result, functionNames };
}

export async function generateAbi({ check = false } = {}) {
  const [main, types, config] = await Promise.all([
    fs.readFile(path.join(root, 'docs/design/dual-dll-abi.md'), 'utf8'),
    fs.readFile(path.join(root, 'docs/design/dual-dll-abi-types.md'), 'utf8'),
    fs.readFile(path.join(root, 'sdk/abi/constants.json'), 'utf8'),
  ]);
  const result = generateHeaders(main, types, JSON.parse(config));
  const directory = path.join(root, 'sdk/include');
  if (!check) await fs.mkdir(directory, { recursive: true });
  for (const [name, content] of Object.entries(result.headers)) {
    const target = path.join(directory, name);
    if (check) {
      if ((await fs.readFile(target, 'utf8')) !== content)
        throw new Error(`ABI头文件漂移：${name}`);
    } else {
      await fs.writeFile(target, content);
    }
  }
  return { functions: result.functionNames.length, headers: Object.keys(result.headers) };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  generateAbi({ check: process.argv.includes('--check') }).then(
    (result) => console.log(JSON.stringify(result)),
    (error) => {
      console.error(error.message);
      process.exitCode = 1;
    },
  );
}
