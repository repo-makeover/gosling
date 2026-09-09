# gosling Desktop App

Native desktop app for gosling built with [Electron](https://www.electronjs.org/) and [ReactJS](https://react.dev/).

# Building and running

gosling uses [Hermit](https://github.com/cashapp/hermit) to manage dependencies, so you will need to have it installed and activated.

```
git clone git@github.com:cephalopod-ai/gosling.git
cd gosling
source ./bin/activate-hermit
cd ui/desktop
pnpm install
pnpm run start
```

## Platform-specific build requirements

### Linux

For building on Linux distributions, you'll need additional system dependencies:

**Debian/Ubuntu:**

```bash
sudo apt install dpkg fakeroot
```

**Arch/Manjaro:**

```bash
sudo pacman -S dpkg fakeroot
```

**Fedora/RHEL:**

```bash
sudo dnf install dpkg-dev fakeroot
```

# Building notes

This is an electron forge app, using vite and react.js. The `gosling` backend (`gosling serve`) runs as multi process binaries on each window/tab similar to chrome.

## Localization catalogs

Run `pnpm i18n:extract` after changing desktop messages. Existing message changes and removals stop for explicit locale review; after resolving each locale, acknowledge them with `pnpm i18n:sync -- --accept-source-changes`.

Synchronization retains replaced files under `.i18n-sync-recovery/`. After reviewing a successful synchronization, run `pnpm i18n:recovery:clean` to remove only successful recovery transactions. Rolled-back, conflicted, malformed, and incomplete transactions are never removed by that command.

Gosling Desktop uses an embedded backend binary at `/Applications/Gosling.app/Contents/Resources/bin/gosling` in installed apps, and `ui/desktop/src/bin/gosling` in source or bundled builds. If a new Rust provider does not show up in the GUI provider picker, rebuild or copy the updated `gosling` backend binary into the desktop bundle and restart the app.

## Building for different platforms

### macOS

`pnpm run bundle:default` will give you a gosling.app/zip which is signed/notarized but only if you set up the env vars as per `forge.config.ts` (you can empty out the section on osxSign if you don't want to sign it) - this will have all defaults.

`pnpm run bundle:preconfigured` will make a gosling.app/zip signed and notarized, but use the following:

```python
            f"        process.env.GOSLING_PROVIDER__TYPE = '{os.getenv("GOSLING_BUNDLE_TYPE")}';",
            f"        process.env.GOSLING_PROVIDER__HOST = '{os.getenv("GOSLING_BUNDLE_HOST")}';",
            f"        process.env.GOSLING_PROVIDER__MODEL = '{os.getenv("GOSLING_BUNDLE_MODEL")}';"
```

This allows you to set for example GOSLING_PROVIDER\_\_TYPE to be "databricks" by default if you want (so when people start gosling.app - they will get that out of the box). There is no way to set an api key in that bundling as that would be a terrible idea, so only use providers that can do oauth (like databricks can), otherwise stick to default gosling.

### Linux

For Linux builds, first ensure you have the required system dependencies installed (see above), then:

1. Build the Rust CLI binary:

```bash
cd ../..  # Go to project root
cargo build --release -p gosling-cli --bin gosling
```

2. Copy the binary to the expected location:

```bash
mkdir -p src/bin
cp ../../target/release/gosling src/bin/
```

3. Build the application:

```bash
# For ZIP distribution (works on all Linux distributions)
pnpm run make --targets=@electron-forge/maker-zip

# For DEB package (Debian/Ubuntu)
pnpm run make --targets=@electron-forge/maker-deb

# For Flatpak (requires flatpak and flatpak-builder)
pnpm run make --targets=@electron-forge/maker-flatpak
```

The built application will be available in:

- ZIP: `out/make/zip/linux/x64/gosling-linux-x64-{version}.zip`
- DEB: `out/make/deb/x64/gosling_{version}_amd64.deb`
- Flatpak: `out/make/flatpak/x86_64/*.flatpak`
- Executable: `out/gosling-linux-x64/gosling`

### Windows

Use the existing Windows build process as documented.
