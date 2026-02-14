# Repo Packager — Rust implementation plan 🚀

## Summary
A cross-platform Rust CLI that packages the repository into a ZIP which exactly mirrors the repo tree while honoring `.gitignore` and user-specified excludes. Focus: correctness, deterministic outputs, and efficient streaming (no large memory usage).

---

## Goals & requirements ✅
- Preserve full directory structure (including empty directories).
- Honor `.gitignore` semantics exactly (use Git when available; fallback to parser).
- Support multiple custom excludes (paths & globs) and shorthands (e.g. exclude a package).
- Stream files into a ZIP (low memory), deterministic by default (sorted entries, stable timestamps optional).
- Cross-platform (Windows/macOS/Linux).
- Fast and well-tested (unit + integration tests).
- Support persistent configuration via `.packagerignore` or `.packager.toml`.

---

## High-level runtime flow (ordered)
1. Parse CLI flags, shorthands, and load config file if present.
2. Detect repo root (git-aware).
3. Build initial candidate list:
   - Prefer `git ls-files -co --exclude-standard -z` when available and appropriate.
   - Otherwise walk filesystem with a gitignore-compliant walker.
4. Normalize repo-root-relative paths (forward slashes).
5. Apply user excludes (prefix + glob matching).
6. Determine directories (including explicit empty directories that remain after filtering).
7. If `--list-only`, print and exit.
8. Stream files + directory entries into a ZIP in a stable order.
9. Compute checksum during ZIP write (if requested).
10. Exit with appropriate status code and optional report.

---

## CLI (recommended)
- `--zip, -z <path>` — output ZIP (default: <repo>-YYYYMMDD_HHMMSS.zip)
- `--output-dir, -o <dir>` — directory for output (default: repo root)
- `--exclude, -e <pattern>` — repeatable (path or glob)
- `--tracked-only, -t` — only git-tracked files
- `--list-only, -l` — print file list and exit
- `--no-packages, -np` — shorthand: exclude `packages/`
- `--ignore-package, -ig <name>` — shorthand: exclude `packages/<name>` (repeatable)
- `--no-empty-dirs` — skip empty directories (default: include them)
- `--deterministic` — stable timestamps + sorted entries
- `--compression <level>` — `fast`, `default`, `best` (default: default)
- `--checksum` — output SHA256 checksum of the ZIP
- `--recurse-submodules` — include git submodules (default: exclude)
- `--quiet, -q` — suppress all output except errors
- `--verbose, -v` — detailed output

Example: `packager -z release.zip -e packages/Odoo -e graphql`

---

## Key implementation details

### Matching & excludes
- Prefer Git for canonical behavior. If Git unavailable, use a gitignore parser (crate `ignore`) to replicate semantics.
- Normalize paths to repo-root relative; compare using prefix matching for directories and `globset` for wildcard patterns.
- Apply excludes early to avoid walking unnecessary paths.

### Git integration details
- Honor `.gitignore`, `.git/info/exclude`, and global `core.excludesFile`.
- Respect `.gitattributes` entries with `export-ignore` flag.
- Submodules: excluded by default; use `--recurse-submodules` to include.

### Configuration file
- Look for `.packagerignore` (same format as `.gitignore`) for persistent excludes.
- Optionally support `.packager.toml` for advanced configuration:
  ```toml
  [defaults]
  exclude = ["packages/Odoo", "graphql"]
  compression = "fast"
  no_empty_dirs = false
  ```

### Directory / empty-folder handling
- Add directory entries with trailing `/` for empty dirs so they appear in ZIPs.
- Include parent directories implicitly when files exist inside them.

### ZIP creation (streaming)
- Use `zip::write::FileOptions` with `CompressionMethod::Deflated`.
- Process files one at a time, never buffer entire file in memory.
- For files > 100MB, consider `Stored` (no compression) to avoid memory spike.
- Compute SHA256 checksum in single pass using `Sha256::update` during write.
- Memory budget: < 10MB regardless of repo size.
- Write entries in sorted order for determinism; allow optional stable timestamp override.

### Edge cases
- Symlinks: do not follow outside repo root; optionally store symlink metadata.
- Non-UTF8 filenames: preserve raw bytes where supported.
- Windows long paths: handle or document requirement.

### Performance
- Walk phase parallelized with `rayon`; ZIP write is serial.
- Use efficient path normalization/caching.
- Progress indicator for large repos via `indicatif`.

---

## Recommended crates
- `clap` — CLI parsing (with derive macros)
- `ignore` — filesystem walk + gitignore semantics
- `globset` — fast glob matching
- `zip` / `zip-rs` + `flate2` — ZIP creation
- `rayon` — parallel iterator for file collection
- `indicatif` — progress bars
- `sha2` — SHA256 checksums
- `serde` + `toml` — config file support
- `git2` or shell out to `git ls-files` (optional)
- `anyhow` / `thiserror` — error handling
- `tracing` / `tracing-subscriber` — structured logging

---

## Project layout (suggested)
```
src/
  main.rs        — CLI + orchestration
  lib.rs         — core APIs
  collector.rs   — git vs fs listing
  excludes.rs    — normalize & match user excludes
  config.rs      — parse .packagerignore / .packager.toml
  zipper.rs      — streaming ZIP writer + empty-dir support
  checksum.rs    — SHA256 streaming checksum
  progress.rs    — progress bar abstraction
  tests/         — unit + integration tests
Cargo.toml
README.md
```

---

## Implementation tasks & rough estimates
1. CLI + repo-root detection — 1 day
2. File collector (git + `ignore` fallback) — 1–2 days
3. Exclude matching (prefix + globset + shorthands) — 0.5 day
4. Config file support (.packagerignore / .packager.toml) — 0.5 day
5. Streaming ZIP writer with empty-dir support — 1 day
6. Checksum + progress indicator — 0.5 day
7. Tests: unit + integration with small repo fixtures — 1 day
8. Deterministic option + logging — 0.5 day

Estimated MVP: 6–8 days

---

## Testing plan
- Unit tests for path normalization & exclude matching.
- Property-based tests (proptest crate) for path handling edge cases.
- Integration tests using temporary repos covering:
  - tracked/untracked/ignored files
  - `--tracked-only`
  - excludes and shorthands
  - empty directories
  - submodules
  - `.gitattributes` with `export-ignore`
- Snapshot tests (insta crate) for ZIP structure verification.
- Benchmarks (criterion crate) for repos with 10k+ files.
- CI runs on Windows/Linux/macOS.

---

## Exit codes
- `0` — Success
- `1` — No files to package
- `2` — I/O error (read/write failure)
- `3` — Invalid arguments
- `4` — Git not found (when git required)

---

## Example pseudocode
```
config = load_config_file()  // .packagerignore / .packager.toml
exclude_patterns = merge(cli_excludes, config_excludes)

files = if git && !trackedOnly:
           git_ls_files_co_exclude_standard()
        else if git && trackedOnly:
           git_ls_files()
        else:
           walk_with_ignorecrate()

files = apply_user_excludes(files, exclude_patterns)

if files.is_empty():
    error("No files to package")
    exit(1)

dirs = collect_all_dirs_under_repo()
empty_dirs = dirs - parents_of(files) - excluded_dirs

if list_only:
    print(sorted(files))
    exit(0)

checksum = Sha256::new()
write_zip_streaming(zip_path, sorted(files), empty_dirs, &mut checksum)

if checksum_flag:
    print("SHA256: {}", checksum.finalize())
```

---

## Why Rust? — short
- Accurate, testable gitignore behavior via crates.
- Strong path and type safety avoids ad-hoc string bugs (like TrimStart issues seen in PowerShell).
- Efficient streaming ZIP APIs and deterministic builds.
- Excellent cross-platform support and single-binary deployment.

---

## Next steps (pick one)
- [ ] I can scaffold the Rust project (Cargo.toml + starter modules).
- [ ] I can implement the collector + zipper core functions first for review.

Tell me which option you'd like and I'll start it.
