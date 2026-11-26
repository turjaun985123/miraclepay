#!/usr/bin/env node
/**
 * MiraclePay load test (Wave 28) — zero-dependency Node script.
 *
 *   node scripts/load-test.mjs [--url http://localhost:8080] \
 *     [--path /health] [--concurrency 20] [--seconds 20] [--allow-remote]
 *
 * Safety: refuses non-localhost targets unless --allow-remote is
 * passed — this is a self-check tool, not a cannon.
 *
 * Reports: requests, errors (non-2xx, counted by status), wall time,
 * req/s, and latency p50/p95/p99/max in ms.
 */

const args = process.argv.slice(2);
function opt(name, fallback) {
  const idx = args.indexOf(`--${name}`);
  return idx >= 0 && args[idx + 1] ? args[idx + 1] : fallback;
}
const baseUrl = opt('url', 'http://localhost:8080').replace(/\/+$/, '');
const path = opt('path', '/health');
const concurrency = Number(opt('concurrency', '20'));
const seconds = Number(opt('seconds', '20'));
const allowRemote = args.includes('--allow-remote');

const host = new URL(baseUrl).hostname;
const isLocal = ['localhost', '127.0.0.1', '::1'].includes(host);
if (!isLocal && !allowRemote) {
  console.error(`Refusing to load-test non-local host "${host}" without --allow-remote.`);
  process.exit(1);
}

const deadline = Date.now() + seconds * 1000;
const latencies = [];
const statusCounts = new Map();
let completed = 0;

async function worker() {
  while (Date.now() < deadline) {
    const started = performance.now();
    try {
      const response = await fetch(`${baseUrl}${path}`);
      await response.text();
      statusCounts.set(response.status, (statusCounts.get(response.status) ?? 0) + 1);
    } catch {
      statusCounts.set(0, (statusCounts.get(0) ?? 0) + 1);
    }
    latencies.push(performance.now() - started);
    completed += 1;
  }
}

const startedAt = performance.now();
await Promise.all(Array.from({ length: concurrency }, worker));
const wallSeconds = (performance.now() - startedAt) / 1000;

latencies.sort((a, b) => a - b);
const pct = (p) => latencies[Math.min(latencies.length - 1, Math.floor((p / 100) * latencies.length))];
const errors = [...statusCounts.entries()]
  .filter(([status]) => status < 200 || status >= 300)
  .reduce((sum, [, count]) => sum + count, 0);

console.log(`Target:        ${baseUrl}${path}`);
console.log(`Concurrency:   ${concurrency}`);
console.log(`Wall time:     ${wallSeconds.toFixed(1)}s`);
console.log(`Requests:      ${completed}`);
console.log(`Throughput:    ${(completed / wallSeconds).toFixed(1)} req/s`);
console.log(`Statuses:      ${[...statusCounts.entries()].map(([s, c]) => `${s}×${c}`).join('  ')}`);
console.log(`Errors:        ${errors} (${((errors / Math.max(1, completed)) * 100).toFixed(2)}%)`);
console.log(
  `Latency ms:    p50 ${pct(50).toFixed(1)}  p95 ${pct(95).toFixed(1)}  p99 ${pct(99).toFixed(1)}  max ${latencies[latencies.length - 1].toFixed(1)}`,
);
