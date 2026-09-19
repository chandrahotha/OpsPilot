'use strict';

/**
 * Tests for native binary resolution.
 *
 * Every lookup is stubbed, so these tests describe the documented resolution
 * order and do not depend on the machine they run on.
 */

const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');

const {
  binaryName,
  bundledBinaryPath,
  developmentBinaryPaths,
  platformPackageName,
  resolveBinary,
} = require('../lib/resolve-binary.js');

const ROOT = path.resolve('/repo/pilot/packages/cli');

test('binaryName adds .exe only on Windows', () => {
  assert.equal(binaryName('win32'), 'pilot.exe');
  assert.equal(binaryName('darwin'), 'pilot');
  assert.equal(binaryName('linux'), 'pilot');
});

test('platformPackageName scopes platform and architecture', () => {
  assert.equal(platformPackageName('win32', 'x64'), '@ops-pilot/cli-win32-x64');
  assert.equal(platformPackageName('darwin', 'arm64'), '@ops-pilot/cli-darwin-arm64');
  assert.equal(platformPackageName('linux', 'x64'), '@ops-pilot/cli-linux-x64');
});

test('development builds are searched upwards to the repository root', () => {
  const candidates = developmentBinaryPaths(ROOT, 'linux');

  assert.deepEqual(candidates.slice(0, 2), [
    path.join(ROOT, 'target', 'release', 'pilot'),
    path.join(ROOT, 'target', 'debug', 'pilot'),
  ]);
  assert.ok(candidates.includes(path.resolve('/repo/target/release/pilot')));
});

test('OPS_PILOT_BINARY wins over every other candidate', () => {
  const override = path.resolve('/custom/pilot');

  const resolution = resolveBinary({
    platform: 'linux',
    env: { OPS_PILOT_BINARY: override },
    packageRoot: ROOT,
    exists: (candidate) => candidate === override,
  });

  assert.equal(resolution.binary, override);
  assert.equal(resolution.source, 'OPS_PILOT_BINARY');
});

test('a wrong OPS_PILOT_BINARY is reported instead of silently ignored', () => {
  const resolution = resolveBinary({
    platform: 'linux',
    env: { OPS_PILOT_BINARY: path.resolve('/custom/pilot') },
    packageRoot: ROOT,
    exists: () => false,
  });

  assert.equal(resolution.binary, undefined);
  assert.match(resolution.error, /OPS_PILOT_BINARY/);
});

test('an installed platform package is preferred over the bundled binary', () => {
  const manifest = path.resolve('/installed/node_modules/@ops-pilot/cli-win32-x64/package.json');
  const expected = path.resolve('/installed/node_modules/@ops-pilot/cli-win32-x64/bin/pilot.exe');

  const resolution = resolveBinary({
    platform: 'win32',
    arch: 'x64',
    env: {},
    packageRoot: ROOT,
    requireResolve: (specifier) =>
      specifier === '@ops-pilot/cli-win32-x64/package.json' ? manifest : null,
    exists: (candidate) => candidate === expected,
  });

  assert.equal(resolution.binary, expected);
  assert.equal(resolution.source, '@ops-pilot/cli-win32-x64');
});

test('the bundled binary is used when no platform package is installed', () => {
  const bundled = bundledBinaryPath(ROOT, 'win32');

  const resolution = resolveBinary({
    platform: 'win32',
    env: {},
    packageRoot: ROOT,
    requireResolve: () => null,
    exists: (candidate) => candidate === bundled,
  });

  assert.equal(resolution.binary, bundled);
  assert.equal(resolution.source, 'bundled in ops-pilot');
});

test('a development build is the last resort', () => {
  const development = path.resolve('/repo/target/release/pilot');

  const resolution = resolveBinary({
    platform: 'linux',
    env: {},
    packageRoot: ROOT,
    requireResolve: () => null,
    exists: (candidate) => candidate === development,
  });

  assert.equal(resolution.binary, development);
  assert.equal(resolution.source, 'development build');
});

test('a missing binary produces an actionable error', () => {
  const resolution = resolveBinary({
    platform: 'linux',
    arch: 'x64',
    env: {},
    packageRoot: ROOT,
    requireResolve: () => null,
    exists: () => false,
  });

  assert.equal(resolution.binary, undefined);
  assert.match(resolution.error, /linux-x64/);
  assert.match(resolution.error, /@ops-pilot\/cli-linux-x64/);
  assert.match(resolution.error, /cargo build --release --bin pilot/);
  assert.ok(resolution.searched.includes(bundledBinaryPath(ROOT, 'linux')));
});