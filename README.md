# reposync-rs

Mirror Git repositories from an origin to a target and emit a Markdown status table. This Rust port keeps the behavior of the original Go [`reposync`](https://github.com/mix-mirror/reposync) while using libgit2 and mimalloc for predictable performance.

## Features

- Mirror all branches and tags origin → target using a temporary bare repo
- HTTP, SSH, or anonymous access; supports tokens, username/password, inline keys, or key paths with optional passphrases
- Parallel syncs with a configurable worker count and optional live fetch/push progress bars
- Generate a Markdown table (stdout) or replace a placeholder inside an existing `.md` file in place
- YAML config with environment/secret indirection (`$VAR`, `env:NAME`, `file:/path`, base64 variants)
- Detect default branch (`main`/`master` preferred; otherwise the most recent head) and sort rows by latest commit time

## Install

Prereqs: Rust (edition 2024; Rust 1.84+ recommended).

```bash
# From a local checkout
cargo install --path .

# Or build without installing
cargo build --release
```

## Quick start

1. Create a config file, e.g. `config.yml`:

   ```yaml
   repos:
   - name: my-repo
       origin:
       url: https://github.com/org/source-repo.git
       auth:
           type: http
           token: env:GITHUB_TOKEN   # prefer env indirection; avoid committing secrets
       target:
       url: git@github.com:org/destination-repo.git
       auth:
           type: ssh
           ssh_private_key_path: ~/.ssh/id_ed25519
   ```

2. Run a sync and print the table:

    ```bash
    reposync-rs --config config.yml
    ```

3. To update a Markdown file in place, add a placeholder token anywhere in it (default: `<!-- REPOSYNC -->`), then run:

    ```bash
    reposync-rs --config config.yml --markdown README.md --placeholder "<!-- REPOSYNC -->"
    ```

## CLI

```text
reposync-rs --config <path> [--markdown <path>] [--placeholder "<token>"] [--concurrency N] [--progress=false]
```

- `--config` (required): path to YAML config
- `--markdown` (optional): Markdown file to rewrite in place; otherwise the table prints to stdout
- `--placeholder` (optional): token to replace in the Markdown file (default `<!-- REPOSYNC -->`)
- `--concurrency` (optional): max concurrent repo syncs (default = available CPUs; minimum 1)
- `--progress` (optional): stream libgit2 fetch/push progress; disable with `--progress=false`

Exit code is non-zero on parse errors, missing placeholder, failed fetch/push, etc.

## Configuration reference

Top-level shape:

```yaml
repos:
  - name: <string> # friendly label; falls back to repo name derived from origin URL
    origin:
      url: <string> # source remote
      auth: # see auth below
    target:
      url: <string> # destination remote
      auth: # see auth below
```

Auth options:

```yaml
auth:
  type: none | http | ssh

  # HTTP
  username: <string>
  password: <string>
  token: <string> # recommended for PATs

  # SSH
  ssh_private_key_path: <path> # e.g. ~/.ssh/id_ed25519
  ssh_private_key: | # inline key (optional alternative)
    -----BEGIN OPENSSH PRIVATE KEY-----
    ...
    -----END OPENSSH PRIVATE KEY-----
  ssh_passphrase: <string> # optional
```

Secret and environment indirection (applies to all string fields):

- `$VAR` or `${VAR}` — expand from environment
- `env:NAME` — value from `os.getenv("NAME")`
- `env-b64:NAME` — base64-decode environment value
- `file:/path/to/secret` — file contents
- `file-b64:/path/to/secret.b64` — base64-decode file contents

## Output

Columns in the generated table:

- Repository (links to target URL when present, otherwise origin)
- Default Branch
- Latest Commit Time (UTC)
- Commit (short SHA)
- Message (first line)
- Last Synced (UTC)

Example:

```markdown
| Repository | Default Branch | Latest Commit Time (UTC) | Commit    | Message        | Last Synced          |
| ---------- | -------------- | ------------------------ | --------- | -------------- | -------------------- |
| my-repo    | main           | 2025-09-12T10:20:30Z     | `abc1234` | Initial import | 2025-09-12T10:21:05Z |
```

Rows are sorted by most recent commit time (empty timestamps fall back to name sort). The default branch is chosen as `main`/`master` when present, otherwise the branch with the most recent commit; if no heads exist, `main` is reported with empty commit data.

## Implementation notes

- Uses Rayon to parallelize repo syncs and keeps results ordered to match the config
- Progress bars are powered by `tracing-indicatif`; disable via `--progress=false`
- Temporary directories are cleaned up after each run; fetch/push uses a bare repo
- Vendored libgit2/OpenSSL mean no system Git or OpenSSL installation is required
- Global allocator is mimalloc for predictable performance

## Development

```bash
cargo test
cargo fmt
cargo clippy
```

## License

MIT; see [`LICENSE`](./LICENSE).
