#!/usr/bin/env node
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const directory = fileURLToPath(new URL('../../docs/design/', import.meta.url));
const allowlist = JSON.parse(readFileSync(fileURLToPath(new URL('./zero-allowlist.json', import.meta.url)), 'utf8'));
if (allowlist.schemaVersion !== 1 || !Array.isArray(allowlist.allow) || allowlist.allow.some((item) => typeof item !== 'string') || new Set(allowlist.allow).size !== allowlist.allow.length) throw new Error('Invalid allowlist schema');
const allow = new Set(allowlist.allow);
const missingBaseline = new Set();
const marked = new Set();
const blocks = (text) => [...text.matchAll(/```c\r?\n([\s\S]*?)```/g)];
const counts = { 'zero=default': 0, 'zero=invalid': 0, explicit: 0 };
let problems = 0;
for (const file of readdirSync(directory).filter((name) => /^dual-dll-abi.*\.md$/.test(name)).sort()) {
  const text = readFileSync(path.join(directory, file), 'utf8');
  for (const block of blocks(text)) {
    const content = block[1];
    const clean = content.replace(/\/\*[\s\S]*?\*\//g, (s) => s.replace(/[^\r\n]/g, ' ')).replace(/\/\/[^\n]*/g, (s) => ' '.repeat(s.length));
    for (const structure of clean.matchAll(/typedef\s+struct\s+\w+\s*\{([\s\S]*?)\}\s*(\w+_t)\s*;/g)) {
      const bodyStart = structure.index + structure[0].indexOf('{') + 1;
      const body = structure[1];
      for (const declaration of body.matchAll(/\b(uint32_t|uint64_t|int32_t|int64_t|size_t|am\w+_e)\s+([^;{}]+);/g)) {
        const type = declaration[1];
        const start = bodyStart + declaration.index;
        const end = start + declaration[0].length;
        const suffix = content.slice(end).match(/^(?:[ \t]*\/\*[\s\S]*?\*\/)*[^\S\r\n]*/)?.[0] ?? '';
        const marker = [...suffix.matchAll(/\/\*\s*(zero\s*=\s*[^*]*?|explicit)\s*\*\//g)].at(-1)?.[1].trim();
        for (const part of declaration[2].split(',')) {
          if (part.includes('*')) continue;
          const field = part.trim().match(/^(\w+)\s*(?:\[[^\]]*\])?\s*$/)?.[1];
          if (!field) continue;
          const line = text.slice(0, block.index).split('\n').length + content.slice(0, start).split('\n').length;
          const label = `${structure[2]}.${field} ${file}:${line}`;
          const key = `${structure[2]}.${field}`;
          if (marker) marked.add(key);
          if (!marker && allow.has(key)) missingBaseline.add(key);
          else if (!marker) { console.log(`MISSING ${label}`); problems++; }
          else if (!(marker in counts)) { console.log(`INVALID ${label} (${marker})`); problems++; }
          else if ((field === 'struct_size' || field === 'abi_version') && marker !== 'explicit') { console.log(`RESERVED-MUST-BE-EXPLICIT ${label}`); problems++; }
          else counts[marker]++;
        }
      }
    }
  }
}
for (const key of allow) {
  if (!missingBaseline.has(key) || marked.has(key)) { console.log(`STALE ${key}`); problems++; }
}
if (!process.argv.includes('--check')) console.log(`Annotations: zero=default ${counts['zero=default']}, zero=invalid ${counts['zero=invalid']}, explicit ${counts.explicit}; baseline ${missingBaseline.size}; problems ${problems}`);
if (process.argv.includes('--check') && problems) process.exitCode = 1;
