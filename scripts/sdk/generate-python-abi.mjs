import { promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseAbiTypes, parseAbiFunctions, parseStatusCodes } from './abi-model.mjs';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
export function pythonType(type) {
  const primitives = {
    u8: 'c_uint8',
    u32: 'c_uint32',
    u64: 'c_uint64',
    i32: 'c_int32',
    i64: 'c_int64',
    f32: 'c_float',
    f64: 'c_double',
    c_char: 'c_char',
    c_void: 'None',
  };
  if (primitives[type]) return primitives[type];
  const array = type.match(/^\[(.*); (\d+)\]$/);
  if (array) return `${pythonType(array[1])} * ${array[2]}`;
  const pointer = type.match(/^\*(?:const|mut) (.*)$/);
  if (pointer)
    return pointer[1] === 'c_void'
      ? 'c_void_p'
      : pointer[1] === 'c_char'
        ? 'c_char_p'
        : `POINTER(${pythonType(pointer[1])})`;
  const callback = type.match(/^Option<unsafe extern "C" fn\((.*?)\)(?: -> (.*))?>$/);
  if (callback)
    return `CFUNCTYPE(${pythonType(callback[2] ?? 'c_void')}${callback[1] ? ', ' + callback[1].split(', ').map(pythonType).join(', ') : ''})`;
  if (!/^am\w+$/.test(type)) throw new Error(`Python不支持的类型：${type}`);
  return type;
}
export async function generatePython({ check = false } = {}) {
  const [main, types, constantText] = await Promise.all([
    fs.readFile(path.join(root, 'docs/design/dual-dll-abi.md'), 'utf8'),
    fs.readFile(path.join(root, 'docs/design/dual-dll-abi-types.md'), 'utf8'),
    fs.readFile(path.join(root, 'sdk/abi/constants.json'), 'utf8'),
  ]);
  const model = parseAbiTypes(main, types);
  const functions = parseAbiFunctions(main);
  const config = JSON.parse(constantText);
  const names = [...new Set([...model.structures.map((s) => s.name), ...model.handles])];
  let result =
    '# 由scripts/sdk/generate-python-abi.mjs生成，与C头文件同源，勿手改。\nfrom ctypes import *\n\n';
  result += names.map((name) => `class ${name}(Structure):\n    pass\n`).join('\n');
  result += '\n' + model.aliases.map((s) => `${s.name} = ${pythonType(s.type)}`).join('\n');
  for (const [name, values] of Object.entries(model.enums))
    result +=
      `\n${name} = c_int32\n` + values.map((v) => `${v.name} = ${v.value}`).join('\n') + '\n';
  result +=
    '\n' + model.callbacks.map((c) => `${c.name} = ${pythonType(c.type)}`).join('\n') + '\n';
  for (const s of model.structures)
    result += `\n${s.name}._fields_ = [\n${s.fields.map((f) => `    ("${f.name}", ${pythonType(f.type)}),`).join('\n')}\n]\n`;
  const constants = {
    ...config.constants,
    ...Object.fromEntries(parseStatusCodes(types).map((s) => [s.constant, s.value])),
  };
  for (const values of Object.values(model.enums))
    for (const entry of values) {
      if (entry.name in constants && constants[entry.name] !== entry.value)
        throw new Error(`枚举与常量数值冲突：${entry.name}`);
      delete constants[entry.name];
    }
  for (const [prefix, values] of [
    ['AMC_CAP_', config.cameraCapabilities],
    ['AMV_CAP_', config.visionCapabilities],
  ])
    values.forEach((name, i) => {
      constants[prefix + name] = 2 ** i;
    });
  result +=
    '\n' +
    Object.entries(constants)
      .map(([name, value]) => `${name} = ${value}`)
      .join('\n');
  result +=
    '\n\nFUNCTIONS = {\n' +
    functions
      .map(
        (f) =>
          `    "${f.name}": (${pythonType(f.returns)}, [${f.parameters.map(pythonType).join(', ')}]),`,
      )
      .join('\n') +
    '\n}\n';
  result +=
    '\ndef bind(library, prefix, require_all=False):\n    """绑定实际导出；开发子集返回缺失清单，最终交付用require_all强制齐全。"""\n    missing = []\n    for name, (result, arguments) in FUNCTIONS.items():\n        if not name.startswith(prefix):\n            continue\n        try:\n            function = getattr(library, name)\n        except AttributeError:\n            missing.append(name)\n            continue\n        function.restype = result\n        function.argtypes = arguments\n    if require_all and missing:\n        raise RuntimeError("SDK symbols missing: " + ", ".join(missing))\n    return missing\n';
  const target = path.join(root, 'sdk/bindings/python/amrt_sdk.py');
  if (check) {
    if ((await fs.readFile(target, 'utf8')) !== result) throw new Error('Python ABI工件漂移');
  } else {
    await fs.mkdir(path.dirname(target), { recursive: true });
    await fs.writeFile(target, result);
  }
  return { structures: model.structures.length, functions: functions.length };
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url))
  generatePython({ check: process.argv.includes('--check') }).then(
    (r) => console.log(JSON.stringify(r)),
    (error) => {
      console.error(error.message);
      process.exitCode = 1;
    },
  );
