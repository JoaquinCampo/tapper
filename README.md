# tapper

See what flows through every stage of a Unix pipeline.

## The problem

You debug shell pipelines by breaking them apart and running each stage one at a time. Tapper does this for you — one command captures the intermediate output at every pipe junction and shows you exactly where data gets transformed, filtered, or lost.

## Demo

```
$ tapper 'echo "hello\nworld\nfoo\nbar\nbaz" | grep -v foo | sort | wc -l'

Pipeline: echo "hello\nworld\nfoo\nbar\nbaz" | grep -v foo | sort | wc -l

● Stage 1: echo "hello\nworld\nfoo\nbar\nbaz"
  → 5 lines (27 B) in 29ms

● Stage 2: grep -v foo
  → 4 lines (23 B) in 38ms  [20.0% filtered]

● Stage 3: sort
  → 4 lines (23 B) in 35ms

● Stage 4: wc -l
  → 1 lines (9 B) in 23ms  [75.0% filtered]

Total: 125ms
```

By default, Tapper launches a TUI with a stage list on the left and a scrollable output viewer on the right. Use `--no-tui` for the plain text output shown above.

## Install

```
cargo install --git https://github.com/JoaquinCampo/tapper
```

Requires Rust 1.70+.

## Usage

```sh
# Basic: debug a pipeline
tapper 'cat access.log | grep 500 | awk "{print \$1}" | sort | uniq -c | sort -rn'

# Plain text output (no TUI)
tapper --no-tui 'du -sh * | sort -rh | head -20'

# Stats only — just line counts, byte counts, timing
tapper --stats 'find . -name "*.rs" | xargs grep TODO | sort'

# Extract the output of a specific stage (0-indexed)
tapper --stage 1 'cat data.csv | cut -d, -f2 | sort -u'
```

### Real-world examples

**Find where data gets lost in a log pipeline:**

```
$ tapper --stats 'cat server.log | grep ERROR | grep -v timeout | awk "{print \$4}" | sort -u'

● Stage 1: cat server.log
  → 84210 lines (12.3 MB) in 52ms

● Stage 2: grep ERROR
  → 1523 lines (198.4 KB) in 41ms  [98.2% filtered]

● Stage 3: grep -v timeout
  → 89 lines (11.2 KB) in 38ms  [94.2% filtered]

● Stage 4: awk "{print $4}"
  → 89 lines (1.1 KB) in 35ms

● Stage 5: sort -u
  → 12 lines (156 B) in 29ms  [86.5% filtered]
```

**Pipe a specific stage's output into another tool:**

```sh
# Get stage 2's output and inspect it further
tapper --stage 2 'cat data.csv | sort -t, -k3 | uniq' | less
```

## TUI keyboard shortcuts

| Key | Action |
|---|---|
| `j` / `k` | Select next/previous stage |
| `J` / `K` | Scroll output down/up |
| `Ctrl-d` / `Ctrl-u` | Page down/up |
| `g` / `G` | Jump to top/bottom |
| `Tab` | Toggle between stdout and stderr |
| `/` | Search in current output |
| `n` / `N` | Next/previous search match |
| `q` / `Esc` | Quit |

## How it works

Tapper parses the pipeline string, splitting on unquoted `|` characters. Each stage is spawned as a subprocess via `sh -c`. The output of stage N is captured into a buffer and then fed as stdin to stage N+1. After all stages complete, Tapper reports line counts, byte counts, timing, and exit codes. Output is capped at 10 MB per stage.

## License

[MIT](LICENSE)
