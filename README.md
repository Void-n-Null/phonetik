# phonetik

A Rust library and HTTP server for English phonetic analysis. Rhyme detection, stress scanning, meter identification, and syllable counting — backed by the full CMU Pronouncing Dictionary compiled directly into the binary.

No files to download. No API keys. No runtime dependencies. `Phonetik::new()` and go.

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
phonetik = "0.1"
```

Or to run the HTTP server:

```
cargo install phonetik
```

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
let scan = ph.scan("shall I compare thee to a summer's day");
println!("{} — {}", scan.visual, scan.meter.name);
// / / x / / / x / x / — iambic pentameter

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

Endpoints: `/health`, `/syllables`, `/syllable-counts`, `/rhymes`, `/rhymes/perfect`, `/rhymes/slant`, `/rhymes/near`, `/compare`, `/scan`, `/rhymemap`

Disable the server feature for library-only use (drops axum, tokio, etc.):

```toml
[dependencies]
phonetik = { version = "0.1", default-features = false }
```

## License

MIT
