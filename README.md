# phonetik

A Rust library and HTTP server for English phonetic analysis. Rhyme detection, stress scanning, meter identification, and syllable counting — backed by the full CMU Pronouncing Dictionary compiled directly into the binary.

No files to download. No API keys. No runtime dependencies. `Phonetik::new()` and go.

[![Phonetik MCP server](https://glama.ai/mcp/servers/Void-n-Null/phonetik/badges/card.svg)](https://glama.ai/mcp/servers/Void-n-Null/phonetik)

## What it does

- **Rhyme finding** — perfect, slant, and near rhymes with confidence scores
- **Stress analysis** — extract stress patterns from any English text
- **Meter detection** — identify iambic pentameter, trochaic tetrameter, etc.
- **Phoneme lookup** — ARPAbet transcriptions for 126K words
- **Syllable counting** — per-word and batch
- **Rhyme map** — detect phoneme repetition patterns across lines of verse
- **Word comparison** — phonetic similarity scoring between any two words

## Install

```toml
[dependencies]
phonetik = "0.3"
```

Or install the binaries:

```
cargo install phonetik
```

This gives you `phonetik-server` (HTTP API) and `phonetik-mcp` (MCP tool server).

## Usage

```rust
use phonetik::Phonetik;

let ph = Phonetik::new();

// Rhymes
for r in ph.rhymes("love", 10) {
    println!("{} ({:?}, {:.0}%)", r.word, r.rhyme_type, r.confidence * 100.0);
}
// ABOVE (Perfect, 100%)
// DOVE (Perfect, 100%)
// GLOVE (Perfect, 100%)
// MOVE (Slant, 60%)
// GROOVE (Slant, 60%)

// Meter
let scan = ph.scan("uneasy lies the head that wears the crown");
println!("{} — {}", scan.visual, scan.meter.name);
// x / x / x / x / x / — iambic pentameter

// Comparison
let cmp = ph.compare("cat", "bat").unwrap();
println!("{:.0}% similar, {:?}", cmp.similarity * 100.0, cmp.rhyme_type);
// 67% similar, Perfect

// Lookup
let info = ph.lookup("extraordinary").unwrap();
println!("{}: {} syllables", info.word, info.syllable_count);
// EXTRAORDINARY: 6 syllables
```

## How it works

The entire CMU Pronouncing Dictionary is compiled into the binary at build time. Every phoneme is encoded as a single byte — no strings in the hot path. Rhyme detection is algorithmic:

- **Perfect rhymes** are resolved by grouping words that share identical sound from their last stressed vowel onward. One hash lookup.
- **Slant rhymes** use a vowel distance matrix and coda suffix matching. Words sharing the same consonant ending but different vowels (love/move) are found by index traversal, not pairwise comparison.
- **Near rhymes** walk a precomputed graph of consonant clusters that differ by one edit. Same vowel, slightly different ending (night/nice).

## Server

The crate includes an HTTP server binary behind the `server` feature (enabled by default):

```
cargo install phonetik
phonetik-server
# Phonetik starting on :1273 [126052 words]
```

Endpoints: `/health`, `/syllables`, `/syllable-counts`, `/rhymes`, `/rhymes/perfect`, `/rhymes/slant`, `/rhymes/near`, `/compare`, `/scan`, `/rhymemap`, `/document`

## MCP

phonetik includes an MCP (Model Context Protocol) server so AI assistants can use it as a tool. Runs over stdio — no network, no auth.

```
cargo install phonetik
```

Add to your MCP client config (Claude Code, Cursor, etc.):

```json
{
  "phonetik": {
    "command": "phonetik-mcp"
  }
}
```

Tools: `lookup`, `rhymes`, `scan`, `compare`, `analyze_document`

## Feature flags

Both `server` and `mcp` are enabled by default. For library-only use:

```toml
[dependencies]
phonetik = { version = "0.3", default-features = false }
```

## License

MIT