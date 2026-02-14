# Packager

A fast, cross-platform CLI tool to package repositories into ZIP archives with full directory structure preservation and `.gitignore` support.

## Features

- **Git-aware**: Honors `.gitignore` semantics using `git ls-files` when available, with filesystem fallback
- **Streaming ZIP**: Low memory usage regardless of repository size
- **Deterministic output**: Optional stable timestamps and sorted entries for reproducible builds
- **Empty directories**: Preserves empty directories in the archive
- **Flexible excludes**: Multiple ways to exclude files (CLI flags, config files, shorthands)
- **SHA256 checksum**: Optional checksum computation during archive creation
- **Cross-platform**: Works on Windows, macOS, and Linux

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/YassaaaTU/packager.git
cd packager

# Build and install
cargo install --path .
```

### Prerequisites

- Rust 1.70 or later
- Git (optional, for git-aware file collection)

## Usage

### Basic Usage

```bash
# Package current directory
packager

# Package a specific directory
packager /path/to/repo

# Output to a specific file
packager -z release.zip

# List files without creating archive
packager --list-only
```

### Exclude Patterns

```bash
# Single exclude
packager -e node_modules

# Multiple excludes (comma-separated)
packager -e "node_modules, target, *.log"

# Multiple -e flags
packager -e node_modules -e target -e "*.log"

# Mixed approach
packager -e "node_modules, target" -e "*.log"
```

### Shorthand Excludes

```bash
# Exclude packages/ directory
packager --no-packages

# Exclude specific package
packager --ignore-package Odoo
packager --ignore-package "Odoo, GraphQL"
```

### Advanced Options

```bash
# Only git-tracked files
packager --tracked-only

# Deterministic output (stable timestamps, sorted entries)
packager --deterministic

# Include git submodules
packager --recurse-submodules

# Output SHA256 checksum
packager --checksum

# Compression level
packager --compression fast    # fast, default, or best

# Skip empty directories
packager --no-empty-dirs

# Quiet mode (errors only)
packager --quiet

# Verbose output
packager --verbose
```

### Full Example

```bash
packager \
  -z release.zip \
  --checksum \
  --deterministic \
  -e "node_modules, target, *.log" \
  --no-packages \
  --compression best
```

## Configuration Files

### .packagerignore

Create a `.packagerignore` file in your repository root (same format as `.gitignore`):

```
# Comment
node_modules/
target/
*.log
.DS_Store
```

### .packager.toml

For more advanced configuration:

```toml
[defaults]
exclude = ["node_modules", "target", "*.log"]
compression = "fast"
no_empty_dirs = false
```

## CLI Reference

| Option | Short | Description |
|--------|-------|-------------|
| `--zip <PATH>` | `-z` | Output ZIP file path (default: `<repo>-YYYYMMDD_HHMMSS.zip`) |
| `--output-dir <DIR>` | `-o` | Directory for output (default: repo root) |
| `--exclude <PATTERN>` | `-e` | Exclude pattern (repeatable, comma-separated) |
| `--tracked-only` | `-t` | Only include git-tracked files |
| `--list-only` | `-l` | Print file list and exit |
| `--no-packages` | | Exclude `packages/` directory |
| `--ignore-package <NAME>` | | Exclude `packages/<name>` (repeatable) |
| `--no-empty-dirs` | | Skip empty directories |
| `--deterministic` | | Stable timestamps and sorted entries |
| `--compression <LEVEL>` | | Compression: `fast`, `default`, `best` |
| `--checksum` | | Output SHA256 checksum of the ZIP |
| `--recurse-submodules` | | Include git submodules |
| `--quiet` | `-q` | Suppress all output except errors |
| `--verbose` | `-v` | Detailed output |

## Exit Codes

| Code | Description |
|------|-------------|
| 0 | Success |
| 1 | No files to package |
| 2 | I/O error |
| 3 | Invalid arguments |
| 4 | Git not found (when required) |

## Development

### Building

```bash
cargo build
cargo build --release
```

### Testing

```bash
cargo test
```

### Running

```bash
cargo run -- --help
cargo run -- --list-only
```

## License

GPL-2.0

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
