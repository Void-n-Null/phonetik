/// Phoneme encoding: every ARPAbet phoneme maps to a single u8.
///
/// Layout:
///   0        = INVALID / padding
///   1..=24   = consonants (B CH D DH F G HH JH K L M N NG P R S SH T TH V W Y Z ZH)
///   25..=39  = vowel bases (AA AE AH AO AW AY EH ER EY IH IY OW OY UH UW)
///   Stressed = base + STRESS_1_OFFSET (40) or + STRESS_2_OFFSET (80)
///
/// So: AA0=25, AA1=65, AA2=105. Max value = 119 (UW2). Fits in 7 bits.
///
/// Properties are derived by lookup table, not branching:
///   IS_VOWEL[id]     — true for any vowel (stressed or unstressed)
///   STRESS_OF[id]    — 0, 1, or 2
///   STRIPPED[id]     — id with stress removed (AA1 → AA0, B → B)
///   TO_STR[id]       — "&str" for JSON serialization at the boundary

// ── Consonant IDs (1..=24) ──────────────────────────────────────────────
pub const B: u8 = 1;
pub const CH: u8 = 2;
pub const D: u8 = 3;
pub const DH: u8 = 4;
pub const F: u8 = 5;
pub const G: u8 = 6;
pub const HH: u8 = 7;
pub const JH: u8 = 8;
pub const K: u8 = 9;
pub const L: u8 = 10;
pub const M: u8 = 11;
pub const N: u8 = 12;
pub const NG: u8 = 13;
pub const P: u8 = 14;
pub const R: u8 = 15;
pub const S: u8 = 16;
pub const SH: u8 = 17;
pub const T: u8 = 18;
pub const TH: u8 = 19;
pub const V: u8 = 20;
pub const W: u8 = 21;
pub const Y: u8 = 22;
pub const Z: u8 = 23;
pub const ZH: u8 = 24;

// ── Vowel base IDs (25..=39) — unstressed (stress 0) ────────────────────
const VOWEL_BASE: u8 = 25;
pub const AA: u8 = 25;
pub const AE: u8 = 26;
pub const AH: u8 = 27;
pub const AO: u8 = 28;
pub const AW: u8 = 29;
pub const AY: u8 = 30;
pub const EH: u8 = 31;
pub const ER: u8 = 32;
pub const EY: u8 = 33;
pub const IH: u8 = 34;
pub const IY: u8 = 35;
pub const OW: u8 = 36;
pub const OY: u8 = 37;
pub const UH: u8 = 38;
pub const UW: u8 = 39;

const VOWEL_END: u8 = 39;
const STRESS_1_OFFSET: u8 = 40;
const STRESS_2_OFFSET: u8 = 80;
const MAX_ID: usize = 120; // UW2 = 39 + 80 = 119

// ── Lookup tables (generated at compile time) ───────────────────────────

/// True if the phoneme is a vowel (any stress level).
pub static IS_VOWEL: [bool; MAX_ID] = {
    let mut t = [false; MAX_ID];
    let mut i = VOWEL_BASE as usize;
    while i <= VOWEL_END as usize {
        t[i] = true; // stress 0
        t[i + STRESS_1_OFFSET as usize] = true; // stress 1
        t[i + STRESS_2_OFFSET as usize] = true; // stress 2
        i += 1;
    }
    t
};

/// Stress level: 0 for consonants and unstressed vowels, 1 or 2 for stressed.
pub static STRESS_OF: [u8; MAX_ID] = {
    let mut t = [0u8; MAX_ID];
    let mut i = VOWEL_BASE as usize;
    while i <= VOWEL_END as usize {
        t[i + STRESS_1_OFFSET as usize] = 1;
        t[i + STRESS_2_OFFSET as usize] = 2;
        i += 1;
    }
    t
};

/// Strip stress: maps any phoneme to its base (unstressed) form.
/// Consonants map to themselves. AA1 → AA0, etc.
pub static STRIPPED: [u8; MAX_ID] = {
    let mut t = [0u8; MAX_ID];
    let mut i = 0;
    while i < MAX_ID {
        if i >= VOWEL_BASE as usize + STRESS_2_OFFSET as usize
            && i <= VOWEL_END as usize + STRESS_2_OFFSET as usize
        {
            t[i] = (i - STRESS_2_OFFSET as usize) as u8;
        } else if i >= VOWEL_BASE as usize + STRESS_1_OFFSET as usize
            && i <= VOWEL_END as usize + STRESS_1_OFFSET as usize
        {
            t[i] = (i - STRESS_1_OFFSET as usize) as u8;
        } else {
            t[i] = i as u8;
        }
        i += 1;
    }
    t
};

/// String representation for JSON serialization. Index by phoneme ID.
static TO_STR: [&str; MAX_ID] = {
    let mut t = [""; MAX_ID];
    // Consonants
    t[1] = "B";
    t[2] = "CH";
    t[3] = "D";
    t[4] = "DH";
    t[5] = "F";
    t[6] = "G";
    t[7] = "HH";
    t[8] = "JH";
    t[9] = "K";
    t[10] = "L";
    t[11] = "M";
    t[12] = "N";
    t[13] = "NG";
    t[14] = "P";
    t[15] = "R";
    t[16] = "S";
    t[17] = "SH";
    t[18] = "T";
    t[19] = "TH";
    t[20] = "V";
    t[21] = "W";
    t[22] = "Y";
    t[23] = "Z";
    t[24] = "ZH";
    // Vowels stress 0
    t[25] = "AA0";
    t[26] = "AE0";
    t[27] = "AH0";
    t[28] = "AO0";
    t[29] = "AW0";
    t[30] = "AY0";
    t[31] = "EH0";
    t[32] = "ER0";
    t[33] = "EY0";
    t[34] = "IH0";
    t[35] = "IY0";
    t[36] = "OW0";
    t[37] = "OY0";
    t[38] = "UH0";
    t[39] = "UW0";
    // Vowels stress 1
    t[65] = "AA1";
    t[66] = "AE1";
    t[67] = "AH1";
    t[68] = "AO1";
    t[69] = "AW1";
    t[70] = "AY1";
    t[71] = "EH1";
    t[72] = "ER1";
    t[73] = "EY1";
    t[74] = "IH1";
    t[75] = "IY1";
    t[76] = "OW1";
    t[77] = "OY1";
    t[78] = "UH1";
    t[79] = "UW1";
    // Vowels stress 2
    t[105] = "AA2";
    t[106] = "AE2";
    t[107] = "AH2";
    t[108] = "AO2";
    t[109] = "AW2";
    t[110] = "AY2";
    t[111] = "EH2";
    t[112] = "ER2";
    t[113] = "EY2";
    t[114] = "IH2";
    t[115] = "IY2";
    t[116] = "OW2";
    t[117] = "OY2";
    t[118] = "UH2";
    t[119] = "UW2";
    t
};

// ── Encode / decode ─────────────────────────────────────────────────────

/// Encode an ARPAbet string ("AH1", "K", "ER0") to a phoneme ID.
/// Returns 0 for unrecognized input.
pub fn encode(s: &str) -> u8 {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if len == 0 {
        return 0;
    }

    // Check if last char is a stress digit
    let last = bytes[len - 1];
    let (base_str, stress) = if last.is_ascii_digit() && len >= 3 {
        (&bytes[..len - 1], last - b'0')
    } else {
        (bytes, 0u8)
    };

    let base = match base_str {
        // Consonants
        b"B" => B,
        b"CH" => CH,
        b"D" => D,
        b"DH" => DH,
        b"F" => F,
        b"G" => G,
        b"HH" => HH,
        b"JH" => JH,
        b"K" => K,
        b"L" => L,
        b"M" => M,
        b"N" => N,
        b"NG" => NG,
        b"P" => P,
        b"R" => R,
        b"S" => S,
        b"SH" => SH,
        b"T" => T,
        b"TH" => TH,
        b"V" => V,
        b"W" => W,
        b"Y" => Y,
        b"Z" => Z,
        b"ZH" => ZH,
        // Vowels
        b"AA" => AA,
        b"AE" => AE,
        b"AH" => AH,
        b"AO" => AO,
        b"AW" => AW,
        b"AY" => AY,
        b"EH" => EH,
        b"ER" => ER,
        b"EY" => EY,
        b"IH" => IH,
        b"IY" => IY,
        b"OW" => OW,
        b"OY" => OY,
        b"UH" => UH,
        b"UW" => UW,
        _ => return 0,
    };

    if base < VOWEL_BASE {
        // Consonant: no stress encoding
        base
    } else {
        match stress {
            0 => base,
            1 => base + STRESS_1_OFFSET,
            2 => base + STRESS_2_OFFSET,
            _ => base,
        }
    }
}

/// Encode a full phoneme string slice into a Vec<u8>.
pub fn encode_all(phonemes: &[&str]) -> Vec<u8> {
    phonemes.iter().map(|s| encode(s)).collect()
}

/// Encode from owned Strings.
pub fn encode_strings(phonemes: &[String]) -> Vec<u8> {
    phonemes.iter().map(|s| encode(s)).collect()
}

/// Decode a phoneme ID to its ARPAbet string. Returns "" for invalid.
#[inline]
pub fn decode(id: u8) -> &'static str {
    if (id as usize) < MAX_ID {
        TO_STR[id as usize]
    } else {
        ""
    }
}

/// Decode a slice of IDs to a Vec of ARPAbet strings (for JSON serialization).
pub fn decode_all(ids: &[u8]) -> Vec<&'static str> {
    ids.iter().map(|&id| decode(id)).collect()
}

/// Decode to owned Strings (for JSON serialization where Serialize needs String).
pub fn decode_to_strings(ids: &[u8]) -> Vec<String> {
    ids.iter().map(|&id| decode(id).to_string()).collect()
}

// ── Inline helpers ──────────────────────────────────────────────────────

/// Check if a phoneme ID is a vowel.
#[inline(always)]
pub fn is_vowel(id: u8) -> bool {
    (id as usize) < MAX_ID && IS_VOWEL[id as usize]
}

/// Get stress level of a phoneme (0, 1, or 2).
#[inline(always)]
pub fn stress(id: u8) -> u8 {
    if (id as usize) < MAX_ID {
        STRESS_OF[id as usize]
    } else {
        0
    }
}

/// Strip stress from a phoneme ID (AA1 → AA0, consonants unchanged).
#[inline(always)]
pub fn strip(id: u8) -> u8 {
    if (id as usize) < MAX_ID {
        STRIPPED[id as usize]
    } else {
        id
    }
}

/// Strip stress from a slice — returns new Vec.
pub fn strip_all(ids: &[u8]) -> Vec<u8> {
    ids.iter().map(|&id| strip(id)).collect()
}

/// Count syllables = count vowels.
pub fn count_syllables(ids: &[u8]) -> usize {
    ids.iter().filter(|&&id| is_vowel(id)).count()
}

/// Extract stress pattern from phonemes (only vowels contribute).
pub fn extract_stresses(ids: &[u8]) -> Vec<i32> {
    ids.iter()
        .filter(|&&id| is_vowel(id))
        .map(|&id| stress(id) as i32)
        .collect()
}

/// Check if a base (stripped) phoneme starts with a vowel letter.
/// Used in rhymemap for vowel-anchored pattern detection.
/// Since stripped IDs 25..=39 are vowels, this is just a range check.
#[inline(always)]
pub fn is_vowel_base(stripped_id: u8) -> bool {
    stripped_id >= VOWEL_BASE && stripped_id <= VOWEL_END
}

/// Validate that an ARPAbet string is a known phoneme.
pub fn is_valid(s: &str) -> bool {
    encode(s) != 0
}

/// Validate base phoneme string (without stress digit).
pub fn is_valid_base(s: &str) -> bool {
    let cleaned = if s.ends_with(|c: char| c.is_ascii_digit()) {
        &s[..s.len() - 1]
    } else {
        s
    };
    encode(cleaned) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Encode/decode roundtrips ────────────────────────────────────────

    #[test]
    fn roundtrip_all_consonants() {
        let consonants = [
            "B", "CH", "D", "DH", "F", "G", "HH", "JH", "K", "L", "M", "N", "NG", "P", "R", "S",
            "SH", "T", "TH", "V", "W", "Y", "Z", "ZH",
        ];
        for &c in &consonants {
            let id = encode(c);
            assert_ne!(id, 0, "{c} should encode to a nonzero ID");
            assert_eq!(decode(id), c, "decode(encode({c})) should roundtrip");
        }
    }

    #[test]
    fn roundtrip_all_vowels_all_stresses() {
        let bases = [
            "AA", "AE", "AH", "AO", "AW", "AY", "EH", "ER", "EY", "IH", "IY", "OW", "OY", "UH",
            "UW",
        ];
        for &b in &bases {
            for stress in 0..=2 {
                let s = format!("{b}{stress}");
                let id = encode(&s);
                assert_ne!(id, 0, "{s} should encode to a nonzero ID");
                assert_eq!(decode(id), s, "decode(encode({s})) should roundtrip");
            }
        }
    }

    #[test]
    fn consonants_have_no_stress_variants() {
        // Encoding "B0", "B1", "B2" should all just produce B (id 1),
        // because consonants ignore the trailing digit — but only if the
        // string is ≥ 3 chars. "B0" is 2 chars, so it goes through the
        // non-digit path and encodes as 0 (unrecognized). This is correct:
        // ARPAbet never appends stress digits to consonants.
        assert_eq!(encode("B"), B);
        assert_eq!(encode("B0"), 0, "B0 is not valid ARPAbet");
    }

    // ── Stress encoding layout ──────────────────────────────────────────

    #[test]
    fn stress_offset_math() {
        assert_eq!(encode("AA0"), AA); // 25
        assert_eq!(encode("AA1"), AA + 40); // 65
        assert_eq!(encode("AA2"), AA + 80); // 105
        assert_eq!(encode("UW0"), UW); // 39
        assert_eq!(encode("UW1"), UW + 40); // 79
        assert_eq!(encode("UW2"), UW + 80); // 119
    }

    #[test]
    fn consonant_ids_are_sequential() {
        assert_eq!(B, 1);
        assert_eq!(ZH, 24);
    }

    // ── Lookup table correctness ────────────────────────────────────────

    #[test]
    fn is_vowel_table_covers_all_stresses() {
        // Stressed vowels
        assert!(IS_VOWEL[encode("AA1") as usize]);
        assert!(IS_VOWEL[encode("EY2") as usize]);
        // Unstressed vowels
        assert!(IS_VOWEL[encode("AH0") as usize]);
        // Consonants
        assert!(!IS_VOWEL[B as usize]);
        assert!(!IS_VOWEL[NG as usize]);
        assert!(!IS_VOWEL[ZH as usize]);
        // Zero (invalid)
        assert!(!IS_VOWEL[0]);
    }

    #[test]
    fn stress_of_table() {
        assert_eq!(STRESS_OF[encode("AA0") as usize], 0);
        assert_eq!(STRESS_OF[encode("AA1") as usize], 1);
        assert_eq!(STRESS_OF[encode("AA2") as usize], 2);
        // Consonants always 0
        assert_eq!(STRESS_OF[B as usize], 0);
        assert_eq!(STRESS_OF[S as usize], 0);
    }

    #[test]
    fn stripped_table() {
        assert_eq!(STRIPPED[encode("AA1") as usize], AA);
        assert_eq!(STRIPPED[encode("AA2") as usize], AA);
        assert_eq!(STRIPPED[encode("AA0") as usize], AA);
        assert_eq!(STRIPPED[encode("UW2") as usize], UW);
        // Consonants map to themselves
        assert_eq!(STRIPPED[B as usize], B);
        assert_eq!(STRIPPED[T as usize], T);
    }

    // ── Helper functions ────────────────────────────────────────────────

    #[test]
    fn is_vowel_fn() {
        assert!(is_vowel(encode("AH1")));
        assert!(is_vowel(encode("IY0")));
        assert!(!is_vowel(B));
        assert!(!is_vowel(0));
        assert!(!is_vowel(255)); // out of range
    }

    #[test]
    fn stress_fn() {
        assert_eq!(stress(encode("AH0")), 0);
        assert_eq!(stress(encode("AH1")), 1);
        assert_eq!(stress(encode("AH2")), 2);
        assert_eq!(stress(K), 0);
        assert_eq!(stress(0), 0);
        assert_eq!(stress(255), 0); // out of range
    }

    #[test]
    fn strip_fn() {
        assert_eq!(strip(encode("AH1")), AH);
        assert_eq!(strip(encode("AH2")), AH);
        assert_eq!(strip(encode("AH0")), AH);
        assert_eq!(strip(K), K); // consonants unchanged
        assert_eq!(strip(255), 255); // out of range passes through
    }

    #[test]
    fn strip_all_mixed_sequence() {
        let input = vec![K, encode("AE1"), T];
        let stripped = strip_all(&input);
        assert_eq!(stripped, vec![K, AE, T]);
    }

    #[test]
    fn count_syllables_by_vowels() {
        // "CAT" → K AE1 T → 1 syllable
        let cat = vec![K, encode("AE1"), T];
        assert_eq!(count_syllables(&cat), 1);

        // "HELLO" → HH AH0 L OW1 → 2 syllables
        let hello = vec![HH, encode("AH0"), L, encode("OW1")];
        assert_eq!(count_syllables(&hello), 2);

        // No vowels → 0
        assert_eq!(count_syllables(&[K, T, S]), 0);

        // Empty → 0
        assert_eq!(count_syllables(&[]), 0);
    }

    #[test]
    fn extract_stresses_from_phonemes() {
        // "HELLO" → HH AH0 L OW1 → stresses [0, 1]
        let hello = vec![HH, encode("AH0"), L, encode("OW1")];
        assert_eq!(extract_stresses(&hello), vec![0, 1]);

        // Only consonants → empty
        assert_eq!(extract_stresses(&[K, T]), Vec::<i32>::new());
    }

    #[test]
    fn is_vowel_base_fn() {
        assert!(is_vowel_base(AA)); // 25
        assert!(is_vowel_base(UW)); // 39
        assert!(!is_vowel_base(B)); // consonant
        assert!(!is_vowel_base(0)); // invalid
                                    // Stressed vowels are NOT base — they have offsets applied
        assert!(!is_vowel_base(encode("AA1")));
    }

    // ── Batch encode/decode ─────────────────────────────────────────────

    #[test]
    fn encode_all_and_decode_to_strings() {
        let input = &["K", "AE1", "T"];
        let encoded = encode_all(input);
        assert_eq!(encoded, vec![K, encode("AE1"), T]);

        let decoded = decode_to_strings(&encoded);
        assert_eq!(decoded, vec!["K", "AE1", "T"]);
    }

    #[test]
    fn decode_all_returns_static_strs() {
        let input = vec![K, encode("AE1"), T];
        let decoded = decode_all(&input);
        assert_eq!(decoded, vec!["K", "AE1", "T"]);

        // Empty input
        assert_eq!(decode_all(&[]), Vec::<&str>::new());
    }

    #[test]
    fn encode_strings_owned() {
        let input = vec!["HH".to_string(), "AH0".to_string()];
        let encoded = encode_strings(&input);
        assert_eq!(encoded, vec![HH, AH]);
    }

    // ── Edge cases ──────────────────────────────────────────────────────

    #[test]
    fn encode_invalid_inputs() {
        assert_eq!(encode(""), 0);
        assert_eq!(encode("GARBAGE"), 0);
        assert_eq!(encode("XX"), 0);
        assert_eq!(encode("A"), 0); // single letter, not a valid phoneme
    }

    #[test]
    fn encode_stress_zero_arm_explicit() {
        // Verify stress 0 encodes to the base ID (not offset)
        assert_eq!(encode("AA0"), AA);
        assert_eq!(encode("UW0"), UW);
        // And that it differs from stress 1 and 2
        assert_ne!(encode("AA0"), encode("AA1"));
        assert_ne!(encode("AA0"), encode("AA2"));
    }

    #[test]
    fn decode_invalid_ids() {
        assert_eq!(decode(0), "");
        assert_eq!(decode(255), "");
        assert_eq!(decode(120), ""); // just past UW2 (119)
    }

    #[test]
    fn boundary_id_120_is_out_of_range() {
        // ID 120 is one past the valid range (UW2 = 119)
        assert!(!is_vowel(120));
        assert_eq!(stress(120), 0);
        assert_eq!(strip(120), 120); // passes through unchanged
    }

    #[test]
    fn stripped_table_stress2_boundary() {
        // UW2 is the highest valid ID (119). Verify the STRIPPED table
        // correctly handles the stress-2 offset math at the boundary.
        let uw2 = encode("UW2");
        assert_eq!(uw2, 119);
        assert_eq!(STRIPPED[uw2 as usize], UW);

        // Also verify AA2 (lowest stress-2 vowel)
        let aa2 = encode("AA2");
        assert_eq!(aa2, 105);
        assert_eq!(STRIPPED[aa2 as usize], AA);
    }

    #[test]
    fn is_valid_known_good_and_bad() {
        assert!(is_valid("K"));
        assert!(is_valid("AA1"));
        assert!(!is_valid(""));
        assert!(!is_valid("NOPE"));
    }

    #[test]
    fn is_valid_base_strips_digit() {
        assert!(is_valid_base("AA1"));
        assert!(is_valid_base("AA"));
        assert!(!is_valid_base("XX3"));
    }

    #[test]
    fn is_valid_base_short_input() {
        // "B" is valid (consonant, no digit to strip)
        assert!(is_valid_base("B"));
        // "B0" — 2 chars, ends with digit, stripped to "B" which is valid
        assert!(is_valid_base("B0"));
        // Single digit — stripped to empty, invalid
        assert!(!is_valid_base("0"));
    }
}
