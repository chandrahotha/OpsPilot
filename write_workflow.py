import os

path = 'd:/DT_Works/Free Tools/Pilot/.github/workflows/release.yml'
os.makedirs(os.path.dirname(path), exist_ok=True)

with open(path, 'w') as f:
    f.write('''name: Build and Release

on:
  push:
    tags:
      - v*
  workflow_dispatch:

permissions:
  contents: write
  packages: write

env:
  CARGO_TERM_COLOR: always

jobs:
  build-binaries:
    name: Build binaries (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            binary: pilot
            asset: pilot-linux-x64
          - os: macos-latest
            target: x86_64-apple-darwin
            binary: pilot
            asset: pilot-macos-x64
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            binary: pilot.exe
            asset: pilot-win32-x64.exe
    steps:
      - uses: actions/checkout@v4
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            target/
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
      - name: Build release binary
        run: cargo build --release --bin pilot --target ${{ matrix.target }}
      - name: Upload binary artifact
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.asset }}
          path: target/${{ matrix.target }}/release/${{ matrix.binary }}
''')

print('Done writing part 1')