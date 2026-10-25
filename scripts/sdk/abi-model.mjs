const strip = (text) => text.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/[^\n]*/g, '');
const blocks = (text) => [...text.matchAll(/```c\r?\n([\s\S]*?)```/g)].map((m) => m[1]).join('\n');
const primitive = {
  void: 'c_void',
  char: 'c_char',
  uint8_t: 'u8',
  uint32_t: 'u32',
  uint64_t: 'u64',
  int32_t: 'i32',
  int64_t: 'i64',
  float: 'f32',
  double: 'f64',
};

export function rustType(cType) {
  const parts = cType.trim().split('*');
  const name = parts[0].replace(/\bconst\b/g, '').trim();
  let result = primitive[name] ?? name;
  if (!/^\w+$/.test(result)) throw new Error(`不支持的C类型：${cType}`);
  for (let i = 0; i < parts.length - 1; i++)
    result = `*${/\bconst\b/.test(parts[i]) ? 'const' : 'mut'} ${result}`;
  return result;
}

function declaration(text) {
  const match = text.trim().match(/^(.*?)([A-Za-z_]\w*)(?:\[(\d+)\])?$/s);
  if (!match || !match[1].trim()) throw new Error(`不支持的C字段：${text}`);
  const base = rustType(match[1]);
  return {
    name: match[2],
    type: match[3] ? `[${base}; ${match[3]}]` : base,
    cType: match[1].trim(),
    array: match[3],
  };
}

function functionType(returnType, args) {
  const parameters =
    args.trim() === 'void' || !args.trim()
      ? []
      : args.split(',').map((arg) => declaration(arg).type);
  const result = returnType.trim() === 'void' ? '' : ` -> ${rustType(returnType)}`;
  return `Option<unsafe extern "C" fn(${parameters.join(', ')})${result}>`;
}

export function parseAbiFunctions(document) {
  return [
    ...strip(blocks(document)).matchAll(/AM_API\s+(.+?)\s+(am[cv]_\w+)\s*\(([\s\S]*?)\);/g),
  ].map((m) => ({
    name: m[2],
    returns: rustType(m[1]),
    parameters:
      m[3].trim() === 'void' ? [] : m[3].split(',').map((parameter) => declaration(parameter).type),
  }));
}

export function parseAbiTypes(mainDocument, typesDocument) {
  const main = strip(blocks(mainDocument));
  let source = strip(blocks(typesDocument));
  const view = main.match(
    /typedef struct am_pixel_view_s\s*\{[\s\S]*?}\s*am_pixel_view_t\s*;/,
  )?.[0];
  if (!view) throw new Error('公共像素视图缺失');
  source += `\n${view}`;
  const structures = [...source.matchAll(/typedef struct (\w+)\s*\{([\s\S]*?)}\s*(\w+)\s*;/g)].map(
    (match) => {
      const fields = [];
      for (const statement of match[2]
        .split(';')
        .map((line) => line.trim())
        .filter(Boolean)) {
        const fn = statement.match(/^(.*?)\(\*(\w+)\)\((.*?)\)$/s);
        if (fn) {
          fields.push({ name: fn[2], type: functionType(fn[1], fn[3]) });
          continue;
        }
        const items = statement.split(',');
        const first = declaration(items[0]);
        fields.push(first);
        for (const item of items.slice(1))
          fields.push(declaration(`${first.cType.replaceAll('*', '')} ${item}`));
      }
      return { tag: match[1], name: match[3], fields };
    },
  );
  const enums = new Map();
  for (const match of (source + '\n' + main).matchAll(
    /typedef enum\s+\w+\s*\{([\s\S]*?)}\s*(\w+)\s*;/g,
  )) {
    if (enums.has(match[2])) continue;
    let next = 0;
    const values = match[1]
      .split(',')
      .map((entry) => entry.trim())
      .filter(Boolean)
      .map((entry) => {
        const item = entry.match(/^(\w+)(?:\s*=\s*(0x[0-9a-fA-F]+|\d+))?$/);
        if (!item) throw new Error(`不支持的枚举：${entry}`);
        if (item[2]) next = Number(item[2]);
        return { name: item[1], value: next++ };
      });
    enums.set(match[2], values);
  }
  const callbacks = [
    ...(source + '\n' + main).matchAll(/typedef\s+(\w+)\s+\(\*(\w+)\)\((.*?)\);/gs),
  ].map((m) => ({ name: m[2], type: functionType(m[1], m[3]) }));
  const aliases = [...source.matchAll(/typedef\s+(int32_t|uint32_t)\s+(\w+)\s*;/g)].map((m) => ({
    name: m[2],
    type: rustType(m[1]),
  }));
  const handles = [...new Set([...main.matchAll(/\b(am[cv]_\w+_t)\b/g)].map((m) => m[1]))];
  return { structures, enums: Object.fromEntries(enums), callbacks, aliases, handles };
}

export function renderRustTypes(model) {
  const known = new Set([
    ...model.structures.map((s) => s.name),
    ...model.callbacks.map((s) => s.name),
    ...model.aliases.map((s) => s.name),
  ]);
  const used = [...model.handles];
  for (const callback of model.callbacks)
    for (const name of callback.type.match(/\bam[cv]_\w+_t\b/g) ?? []) used.push(name);
  const opaque = [...new Set(used)].filter((name) => !known.has(name));
  let base = model.aliases.map((s) => `pub type ${s.name} = ${s.type};`).join('\n');
  base +=
    '\n' + opaque.map((name) => `#[repr(C)]\npub struct ${name} { _opaque: [u8; 0] }`).join('\n');
  for (const [name, values] of Object.entries(model.enums)) {
    base += `\npub type ${name} = i32;\n`;
    base += values.map((value) => `pub const ${value.name}: ${name} = ${value.value};`).join('\n');
  }
  base += '\n' + model.callbacks.map((s) => `pub type ${s.name} = ${s.type};`).join('\n');
  base +=
    "\n/// 只复制完整字段，避免输出C布局中的未初始化填充字节。\npub trait AbiLayout { const FIELDS: &'static [(usize,usize)]; }\n";
  const files = { 'base.rs': base };
  const flatten = (structure, prefix = '') =>
    structure.fields.flatMap((field) => {
      const offset = `${prefix ? prefix + ' + ' : ''}core::mem::offset_of!(${structure.name}, ${field.name})`;
      const nested = model.structures.find((s) => s.name === field.type);
      return nested
        ? flatten(nested, offset)
        : [`(${offset}, core::mem::size_of::<${field.type}>())`];
    });
  for (let i = 0; i < model.structures.length; i += 10) {
    files[`fields_${i / 10}.rs`] = model.structures
      .slice(i, i + 10)
      .map(
        (s) =>
          `impl AbiLayout for ${s.name} { const FIELDS: &'static [(usize,usize)] = &[${flatten(s).join(',')}]; }`,
      )
      .join('\n');
  }
  for (let i = 0; i < model.structures.length; i += 10) {
    files[`types_${i / 10}.rs`] = model.structures
      .slice(i, i + 10)
      .map((s) => {
        const fields = s.fields.map((f) => `    pub ${f.name}: ${f.type},`).join('\n');
        if (!s.fields.some((f) => f.name === 'struct_size'))
          return `#[repr(C)]\n#[derive(Clone, Copy, Default)]\npub struct ${s.name} {\n${fields}\n}`;
        const defaults = s.fields
          .map(
            (f) =>
              `            ${f.name}: ${f.name === 'struct_size' ? 'core::mem::size_of::<Self>() as u32' : f.array ? '[Default::default(); ' + f.array + ']' : 'Default::default()'},`,
          )
          .join('\n');
        return `#[repr(C)]\n#[derive(Clone, Copy)]\npub struct ${s.name} {\n${fields}\n}\nimpl Default for ${s.name} {\n    fn default() -> Self {\n        Self {\n${defaults}\n        }\n    }\n}`;
      })
      .join('\n');
  }
  return files;
}

export function cLayoutProbe(model) {
  const lines = model.structures.map((s) => {
    const offsets = s.fields
      .map((f) => `printf(",%zu", offsetof(${s.name}, ${f.name}));`)
      .join('\n');
    return `printf("${s.name},%zu,%zu", sizeof(${s.name}), _Alignof(${s.name}));\n${offsets}\nprintf("\\n");`;
  });
  return `#include <stdio.h>\n#include "am_sdk_types.h"\nint main(void) {\n${lines.join('\n')}\nreturn 0;\n}\n`;
}

export function parseLayout(output, model) {
  const result = {};
  for (const line of output.trim().split(/\r?\n/)) {
    const [name, ...values] = line.split(',');
    const structure = model.structures.find((s) => s.name === name);
    if (
      !structure ||
      values.length !== structure.fields.length + 2 ||
      values.some((v) => !/^\d+$/.test(v))
    )
      throw new Error(`C布局探针输出非法：${line}`);
    result[name] = {
      size: Number(values[0]),
      align: Number(values[1]),
      offsets: Object.fromEntries(structure.fields.map((f, i) => [f.name, Number(values[i + 2])])),
    };
  }
  if (Object.keys(result).length !== model.structures.length) throw new Error('C布局探针缺少结构');
  return result;
}

export function parseStatusCodes(document) {
  const section = document.split('### 6.2 ')[1];
  if (!section) throw new Error('错误码表缺失');
  const result = [];
  for (const line of section.split(/\r?\n/)) {
    const cells = line.split('|').map((cell) => cell.trim());
    if (!['sdk', 'camera', 'vision'].includes(cells[1])) continue;
    const names = cells[2].split('/').map((name) => name.trim());
    const values = cells[3].split('（')[0].trim();
    const range = values.match(/^(\d+)[–-](\d+)$/);
    const numbers = range
      ? Array.from(
          { length: Number(range[2]) - Number(range[1]) + 1 },
          (_, i) => Number(range[1]) + i,
        )
      : values.split('/').map((value) => Number(value.trim()));
    if (numbers.length !== names.length || numbers.some((n) => !Number.isInteger(n)))
      throw new Error(`错误码数值未明确：${line}`);
    names.forEach((name, i) =>
      result.push({
        name: `${cells[1]}.${name}`,
        value: numbers[i],
        constant:
          `AM_ERR_${cells[1]}_${name.replace(/[A-Z]/g, (letter) => '_' + letter)}`.toUpperCase(),
      }),
    );
  }
  if (new Set(result.map((s) => s.value)).size !== result.length) throw new Error('错误码数值重复');
  return result;
}
