#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { parseAbiTypes } from './abi-model.mjs';

const read = (name) => readFileSync(fileURLToPath(new URL(name, import.meta.url)), 'utf8');
const model = parseAbiTypes(read('../../docs/design/dual-dll-abi.md'), read('../../docs/design/dual-dll-abi-types.md'));
const coverage = JSON.parse(read('./bindings-coverage.json'));
const required = ['amv_init_params_t', 'amc_init_params_t', 'am_image_input_t', 'am_frame_sub_desc_t'];
const issues = [];
if (coverage.schemaVersion !== 1 || !coverage.structures || typeof coverage.structures !== 'object') throw new Error('Invalid coverage schema');
for (const name of required) if (!coverage.structures[name]) issues.push(`UNDECLARED ${name}`);
let count = 0;
for (const [name, entry] of Object.entries(coverage.structures)) {
  const structure = model.structures.find((item) => item.name === name);
  if (!structure) { issues.push(`STALE ${name}`); continue; }
  const fields = entry?.fields ?? {};
  for (const field of structure.fields) if (!Object.hasOwn(fields, field.name)) issues.push(`UNDECLARED ${name}.${field.name}`);
  for (const [field, state] of Object.entries(fields)) {
    count++;
    if (!structure.fields.some((item) => item.name === field)) issues.push(`STALE ${name}.${field}`);
    if (!state || !['exposed', 'defaulted', 'not-applicable'].includes(state.python) || !['exposed', 'defaulted', 'not-applicable'].includes(state.csharp) || typeof state.note !== 'string' || !state.note.trim()) issues.push(`INVALID ${name}.${field}`);
  }
}
if (!process.argv.includes('--check')) console.log(`Bindings coverage: ${Object.keys(coverage.structures).length} structures, ${count} fields, ${issues.length} issues`);
for (const issue of issues) console.error(issue);
if (issues.length) process.exitCode = 1;
