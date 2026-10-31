# portwatch

A small, keyboard-first port monitor for Linux, written in Rust with Ratatui.

It reads `/proc/net` directly, maps socket inodes to processes through `/proc/<pid>/fd`, and never needs `sudo` just to render the list.

## Install

The install script builds the release binary and puts it in `~/.local/bin`.

```bash
./install.sh
portwatch
```

From GitHub:

```bash
git clone https://github.com/<your-user>/portwatch.git
cd portwatch
./install.sh
portwatch
```

The source is downloaded from GitHub; Ratatui and Crossterm are resolved through Cargo's registry.

## Run locally

```bash
cargo run --release
```

Keys:

- `↑` / `↓` or `j` / `k` — move through sockets
- `/` — filter by protocol, address, PID, process, or command
- `r` — refresh immediately
- `x` — ask to send `SIGTERM` to the selected process
- `q` / `Esc` — quit

The first build needs network access to GitHub and Cargo's registry.
