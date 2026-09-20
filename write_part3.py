import os

path = 'd:/DT_Works/Free Tools/Pilot/.github/workflows/release.yml'

part3 = '''

  # Build and test main npm package
  build-npm:
    name: Build npm package
    runs-on: ubuntu-latest
    needs: build-binaries
    steps:
      - uses: actions/checkout@v4
      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'
          registry-url: 'https://registry.npmjs.org'
          cache: 'npm'
      - name: Download binaries
        uses: actions/download-artifact@v4
        with:
          path: artifacts
      - name: Bundle binary into CLI package
        run: |
          # Bundle the native binary into the CLI npm package for fallback
          mkdir -p pilot/packages/cli/bin
          
          # Include all platform binaries with platform-specific names
          cp artifacts/pilot-linux-x64 pilot/packages/cli/bin/pilot-linux-x64
          cp artifacts/pilot-macos-x64 pilot/packages/cli/bin/pilot-darwin-x64
          cp artifacts/pilot-win32-x64.exe pilot/packages/cli/bin/pilot-win32-x64.exe
          
          # Also include the current platform binary as 'pilot' for local installs
          cp artifacts/pilot-linux-x64 pilot/packages/cli/bin/pilot
      - name: Install npm dependencies
        run: npm ci
      - name: Build workspace
        run: npm run build
      - name: Run tests
        run: npm test
      - name: Pack npm package
        run: |
          cd pilot/packages/cli
          npm pack --dry-run 2>&1 | head -20
          npm pack
          mv *.tgz ../../../packages/
      - name: Upload npm package
        uses: actions/upload-artifact@v4
        with:
          name: ops-pilot-npm
          path: packages/*.tgz

  # Create GitHub Release
  create-release:
    name: Create GitHub Release
    runs-on: ubuntu-latest
    needs: [build-binaries, build-tauri, create-platform-packages, build-npm]
    if: startsWith(github.ref, 'refs/tags/v')
    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: release-assets
      - name: Prepare release assets
        run: |
          mkdir -p release
          
          # Native CLI binaries
          for asset in pilot-linux-x64 pilot-macos-x64 pilot-win32-x64.exe; do
            cp release-assets/$asset/$asset release/
          done
          
          # Tauri desktop bundles
          for dir in release-assets/ops-pilot-desktop-*/; do
            if [ -d "$dir" ]; then
              cp -r "$dir"* release/
            fi
          done
          
          # Platform npm packages
          if [ -d "release-assets/platform-packages/packages" ]; then
            cp -r release-assets/platform-packages/packages release/platform-packages
          fi
          
          # Main npm package
          if [ -f "release-assets/ops-pilot-npm/ops-pilot-*.tgz" ]; then
            cp release-assets/ops-pilot-npm/ops-pilot-*.tgz release/
          fi
          
          ls -la release/
      - name: Create Release
        uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ github.ref_name }}
          name: OpsPilot ${{ github.ref_name }}
          draft: false
          prerelease: false
          generate_release_notes: true
          files: release/*
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}

  # Publish to npm
  publish-npm:
    name: Publish to npm
    runs-on: ubuntu-latest
    needs: [build-npm, create-release]
    if: startsWith(github.ref, 'refs/tags/v')
    steps:
      - uses: actions/checkout@v4
      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: '20'
          registry-url: 'https://registry.npmjs.org'
      - name: Download npm package
        uses: actions/download-artifact@v4
        with:
          name: ops-pilot-npm
          path: npm-package
      - name: Publish main CLI package
        run: |
          cd pilot/packages/cli
          npm publish ../*.tgz --access public
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
      - name: Publish platform packages
        run: |
          for pkg in ../platform-packages/*/; do
            if [ -f "$pkg/package.json" ]; then
              echo "Publishing $(basename $pkg)..."
              npm publish "$pkg" --access public
            fi
          done
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
'''

with open(path, 'a') as f:
    f.write(part3)

print('Part 3 done')