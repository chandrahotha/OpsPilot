import os

path = 'd:/DT_Works/Free Tools/Pilot/.github/workflows/release.yml'

part2 = '''

  # Build Tauri desktop app
  build-tauri:
    name: Build Tauri (${{ matrix.os }})
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: linux
          - os: macos-latest
            target: darwin
          - os: windows-latest
            target: win32
    steps:
      - uses: actions/checkout@v4
      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: 'npm'
      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
      - name: Install system dependencies (Linux)
        if: matrix.os == 'ubuntu-latest'
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf
      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            target/
          key: ${{ runner.os }}-tauri-cargo-${{ hashFiles('**/Cargo.lock') }}
      - name: Install npm dependencies
        working-directory: pilot/apps/desktop
        run: npm ci
      - name: Build Tauri app
        working-directory: pilot/apps/desktop
        run: npm run build
      - name: Upload Tauri artifacts
        uses: actions/upload-artifact@v4
        with:
          name: ops-pilot-desktop-${{ matrix.target }}
          path: pilot/apps/desktop/src-tauri/target/release/bundle/

  # Create platform-specific npm packages
  create-platform-packages:
    name: Create platform npm packages
    runs-on: ubuntu-latest
    needs: build-binaries
    steps:
      - uses: actions/checkout@v4
      - name: Download all binaries
        uses: actions/download-artifact@v4
        with:
          path: artifacts
      - name: Create platform packages
        run: |
          mkdir -p packages
          
          # Windows x64
          mkdir -p packages/cli-win32-x64/bin
          cp artifacts/pilot-win32-x64.exe packages/cli-win32-x64/bin/pilot.exe
          
          # macOS x64
          mkdir -p packages/cli-darwin-x64/bin
          cp artifacts/pilot-macos-x64 packages/cli-darwin-x64/bin/pilot
          chmod +x packages/cli-darwin-x64/bin/pilot
          
          # Linux x64
          mkdir -p packages/cli-linux-x64/bin
          cp artifacts/pilot-linux-x64 packages/cli-linux-x64/bin/pilot
          chmod +x packages/cli-linux-x64/bin/pilot
          
          # Create package.json for each platform package
          for platform in win32-x64 darwin-x64 linux-x64; do
            cat > packages/cli-${platform}/package.json << EOF
            {
              "name": "@ops-pilot/cli-${platform}",
              "version": "${{ github.ref_name }}",
              "description": "Native pilot binary for ${platform}",
              "license": "MIT",
              "os": ["${platform%%-x64}"],
              "cpu": ["x64"],
              "files": ["bin"],
              "private": true
            }
            EOF
          done
          
          echo "Created platform packages in packages/"
      - name: Upload platform packages
        uses: actions/upload-artifact@v4
        with:
          name: platform-packages
          path: packages/
'''

with open(path, 'a') as f:
    f.write(part2)

print('Part 2 done')