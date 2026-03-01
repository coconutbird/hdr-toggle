# hdr-toggle

A simple CLI tool to toggle HDR mode on Windows displays.

## Usage

```bash
# Toggle HDR on/off
hdr-toggle --toggle

# Turn HDR on
hdr-toggle --on

# Turn HDR off
hdr-toggle --off

# Set your preferred mode (saves to config)
hdr-toggle --set-preferred on

# Restore to your preferred mode
hdr-toggle --restore

# Check current status
hdr-toggle --get-status
```

## Installation

Download the latest release from [Releases](https://github.com/coconutbird/hdr-toggle/releases) or build from source:

```bash
cargo build --release
```

## Config

Preferred mode is saved to `%APPDATA%\hdr-toggle\config.json`

