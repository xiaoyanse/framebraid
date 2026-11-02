#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { parseAbiFunctions } from './abi-model.mjs';

const read = (name) => readFileSync(fileURLToPath(new URL(name, import.meta.url)), 'utf8');
const main = read('../../docs/design/dual-dll-abi.md');
const constants = JSON.parse(read('../../sdk/abi/constants.json'));
const allowlist = JSON.parse(read('./doc-coverage-allowlist.json'));
if (allowlist.schemaVersion !== 1 || !Array.isArray(allowlist.allow)) throw new Error('Invalid allowlist schema');
const symbols = [...new Set([
  ...parseAbiFunctions(main).map((item) => item.name),
  ...constants.cameraCapabilities.map((name) => `AMC_CAP_${name}`),
  ...constants.visionCapabilities.map((name) => `AMV_CAP_${name}`),
])];
const documents = ['../../sdk/INTEGRATION.md', '../../README.md', '../../README.zh-CN.md', '../../docs/SUPPORT.md'].map(read);
const allow = new Set(allowlist.allow);
const missing = symbols.filter((symbol) => !allow.has(symbol) && !documents.some((doc) => new RegExp(`\\b${symbol}\\b`).test(doc)));
const stale = [...allow].filter((symbol) => !symbols.includes(symbol));
if (!process.argv.includes('--check')) console.log(`Documentation coverage: ${symbols.length} symbols, ${allow.size} allowed, ${missing.length} unmentioned, ${stale.length} stale allowances`);
for (const symbol of missing) console.log(`UNMENTIONED ${symbol}`);
for (const symbol of stale) console.error(`STALE ${symbol}`);
if (missing.length || stale.length) process.exitCode = 1;
