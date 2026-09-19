#!/usr/bin/env node
'use strict';

/**
 * OpsPilot launcher.
 *
 * Runs the native `pilot` binary with the arguments and working directory of the
 * caller, so `cd my-project && pilot` scans that project. Arguments, output and
 * the exit code are passed through untouched.
 */

const { spawn } = require('node:child_process');
const { resolveBinary } = require('../lib/resolve-binary.js');

const resolution = resolveBinary();

if (resolution.error) {
  process.stderr.write(`${resolution.error}\n`);
  process.exit(1);
}

const child = spawn(resolution.binary, process.argv.slice(2), { stdio: 'inherit' });

child.on('error', (error) => {
  process.stderr.write(
    `OpsPilot: failed to start ${resolution.binary} (${resolution.source}): ${error.message}\n`,
  );
  process.exit(1);
});

child.on('exit', (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }

  process.exit(code ?? 1);
});