'use strict';

/**
 * Locate the native `pilot` executable that backs the npm launcher.
 *
 * The npm package itself is a thin Node shim: the real work lives in the Rust
 * binary. Resolution order (most specific first):
 *
 *   1. OPS_PILOT_BINARY      - explicit override, for unusual setups and tests
 *   2. @ops_pilot/cli-<platform>-<arch>  - optional platform package (the way a
 *      published release ships the native binary, `npm i -g ops_pilot @ops_pilot/cli-win32-x64`)
 *   3. <package>/bin/pilot-<platform>-<arch>[.exe] - binary bundled into the package by CI at publish time
 *   4. <repo>/target/{release,debug}/pilot[.exe] - development builds, searched upwards
 *
 * Every lookup is injectable so the resolution logic can be tested without
 * touching the filesystem or the machine configuration.
 */

const fs = require('node:fs');
const path = require('node:path');

/** Native executable name for a platform */
function binaryName(platform) {
  return platform === 'win32' ? 'pilot.exe' : 'pilot';
}

/** Name of the optional package that carries the native binary */
function platformPackageName(platform, arch) {
  return `@ops_pilot/cli-${platform}-${arch}`;
}

/** Binary bundled inside the npm package itself, named for its platform and arch */
function bundledBinaryPath(packageRoot, platform, arch) {
  const suffix = platform === 'win32' ? '.exe' : '';
  return path.join(packageRoot, 'bin', `pilot-${platform}-${arch}${suffix}`);
}

/** Development builds, from the package directory up to the repository root */
function developmentBinaryPaths(packageRoot, platform) {
  const name = binaryName(platform);
  const candidates = [];
  let directory = path.resolve(packageRoot);

  for (let depth = 0; depth < 5; depth += 1) {
    candidates.push(path.join(directory, 'target', 'release', name));
    candidates.push(path.join(directory, 'target', 'debug', name));

    const parent = path.dirname(directory);

    if (parent === directory) {
      break;
    }

    directory = parent;
  }

  return candidates;
}

/** Default existence check: a regular file */
function defaultExists(candidate) {
  try {
    return fs.statSync(candidate).isFile();
  } catch {
    return false;
  }
}

/** Default resolver for an installed package manifest */
function defaultRequireResolve(specifier) {
  try {
    return require.resolve(specifier);
  } catch {
    return null;
  }
}

/**
 * Resolve the native binary.
 *
 * Returns `{ binary, source }` on success, or `{ error, searched }` when no
 * candidate exists. The error is explicit about what was searched so a failed
 * install can be diagnosed without guessing.
 */
function resolveBinary(options = {}) {
  const platform = options.platform ?? process.platform;
  const arch = options.arch ?? process.arch;
  const env = options.env ?? process.env;
  const packageRoot = options.packageRoot ?? path.resolve(__dirname, '..');
  const exists = options.exists ?? defaultExists;
  const requireResolve = options.requireResolve ?? defaultRequireResolve;

  const override = env.OPS_PILOT_BINARY;

  if (override) {
    if (exists(override)) {
      return { binary: override, source: 'OPS_PILOT_BINARY' };
    }

    return {
      error: `OpsPilot: OPS_PILOT_BINARY points to "${override}", which is not an existing file.`,
      searched: [override],
    };
  }

  const searched = [];

  const packageName = platformPackageName(platform, arch);
  const manifest = requireResolve(`${packageName}/package.json`);

  if (manifest) {
    const candidate = path.join(path.dirname(manifest), 'bin', binaryName(platform));
    searched.push(candidate);

    if (exists(candidate)) {
      return { binary: candidate, source: packageName };
    }
  } else {
    searched.push(`${packageName} (not installed)`);
  }

  const bundled = bundledBinaryPath(packageRoot, platform, arch);
  searched.push(bundled);

  if (exists(bundled)) {
    return { binary: bundled, source: 'bundled in ops_pilot' };
  }

  const development = developmentBinaryPaths(packageRoot, platform);
  searched.push(...development);

  const foundDevelopment = development.find((candidate) => exists(candidate));

  if (foundDevelopment) {
    return { binary: foundDevelopment, source: 'development build' };
  }

  return {
    error: [
      `OpsPilot: could not find the native "pilot" executable for ${platform}-${arch}.`,
      '',
      'Searched:',
      ...searched.map((candidate) => `  - ${candidate}`),
      '',
      'Install a published build (npm install -g ops_pilot) or build it from source:',
      '  cargo build --release --bin pilot',
    ].join('\n'),
    searched,
  };
}

module.exports = {
  binaryName,
  bundledBinaryPath,
  developmentBinaryPaths,
  platformPackageName,
  resolveBinary,
};