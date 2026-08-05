# Leaf

A minimal text editor.

[![Latest Release](https://img.shields.io/github/v/release/digitarhythm/leaf?style=flat-square)](https://github.com/digitarhythm/leaf/releases/latest)

## Download

| Platform | Download |
|----------|----------|
| macOS (Apple Silicon) | [Leaf_aarch64.dmg](https://github.com/digitarhythm/leaf/releases/latest/download/Leaf_aarch64.dmg) |
| macOS (Intel) | [Leaf_x64.app.zip](https://github.com/digitarhythm/leaf/releases/latest/download/Leaf_x64.app.zip) |
| Windows (installer) | [Leaf_x64-setup.exe](https://github.com/digitarhythm/leaf/releases/latest/download/Leaf_x64-setup.exe) |
| Linux x86_64 (AppImage) | [Leaf_amd64.AppImage](https://github.com/digitarhythm/leaf/releases/latest/download/Leaf_amd64.AppImage) |
| Linux x86_64 (deb) | [Leaf_amd64.deb](https://github.com/digitarhythm/leaf/releases/latest/download/Leaf_amd64.deb) |
| Linux ARM64 / Chromebook (deb) | [Leaf_arm64.deb](https://github.com/digitarhythm/leaf/releases/latest/download/Leaf_arm64.deb) |
| Linux ARM64 / Chromebook (AppImage) | [Leaf_arm64.AppImage](https://github.com/digitarhythm/leaf/releases/latest/download/Leaf_arm64.AppImage) |

[→ All releases](https://github.com/digitarhythm/leaf/releases)

### Chromebook (ARM)

Install the `.deb` package in the Linux development environment (Crostini):

```sh
sudo apt install ./Leaf_arm64.deb
```

Requires Debian bookworm or later (`libwebkit2gtk-4.1-0`). The `.deb` package is
recommended over the AppImage, which needs FUSE and may not run under Crostini.
