use serde::Serialize;

pub struct MeterDetector;

struct Foot {
    name: &'static str,
    pattern: &'static [i32],
}

const FEET: &[Foot] = &[
    Foot {
        name: "iamb",
        pattern: &[0, 1],
    },
    Foot {
        name: "trochee",
        pattern: &[1, 0],
    },
    Foot {
        name: "anapest",
        pattern: &[0, 0, 1],
    },
    Foot {
        name: "dactyl",
        pattern: &[1, 0, 0],
    },
    Foot {
        name: "spondee",
        pattern: &[1, 1],
    },
    Foot {
        name: "pyrrhic",
        pattern: &[0, 0],
    },
];

const FOOT_COUNTS: &[&str] = &[
    "",
    "monometer",
    "dimeter",
    "trimeter",
    "tetrameter",
    "pentameter",
    "hexameter",
    "heptameter",
    "octameter",
];

impl MeterDetector {
    pub fn detect(binary: &[i32]) -> MeterResult {
        if binary.is_empty() {
            return MeterResult {
                foot_type: "none".into(),
                foot_count: 0,
                meter_name: "none".into(),
                regularity: 0.0,
                expected_pattern: vec![],
                deviations: vec![],
            };
        }

        let mut best = score_foot("iamb", &[0, 1], binary);
        for foot in FEET {
            let candidate = score_foot(foot.name, foot.pattern, binary);
            if candidate.regularity > best.regularity {
                best = candidate;
            }
        }
        best
    }

    pub fn compare(binary_a: &[i32], binary_b: &[i32]) -> MeterComparison {
        let meter_a = Self::detect(binary_a);
        let meter_b = Self::detect(binary_b);
        let same_meter = meter_a.foot_type == meter_b.foot_type;
        let same_length = meter_a.foot_count == meter_b.foot_count;

        let min_len = binary_a.len().min(binary_b.len());
        let max_len = binary_a.len().max(binary_b.len());
        let mut matches = 0;
        let mut mismatches = Vec::new();

        for i in 0..min_len {
            if binary_a[i] == binary_b[i] {
                matches += 1;
            } else {
                mismatches.push(i);
            }
        }
        for i in min_len..max_len {
            mismatches.push(i);
        }

        let agreement = if max_len > 0 {
            matches as f64 / max_len as f64
        } else {
            1.0
        };

        MeterComparison {
            same_meter,
            same_length,
            stress_agreement: (agreement * 10000.0).round() / 10000.0,
            mismatch_positions: mismatches,
        }
    }
}

fn score_foot(name: &str, foot: &[i32], binary: &[i32]) -> MeterResult {
    let foot_len = foot.len();
    let num_feet = binary.len() / foot_len;
    let remainder = binary.len() % foot_len;

    if num_feet == 0 {
        return MeterResult {
            foot_type: name.into(),
            foot_count: 0,
            meter_name: name.into(),
            regularity: 0.0,
            expected_pattern: vec![],
            deviations: vec![],
        };
    }

    let expected: Vec<i32> = (0..binary.len()).map(|i| foot[i % foot_len]).collect();
    let mut matches = 0;
    let mut deviations = Vec::new();

    for i in 0..binary.len() {
        if binary[i] == expected[i] {
            matches += 1;
        } else {
            deviations.push(Deviation {
                position: i,
                expected: if expected[i] == 1 {
                    "stressed"
                } else {
                    "unstressed"
                }
                .into(),
                actual: if binary[i] == 1 {
                    "stressed"
                } else {
                    "unstressed"
                }
                .into(),
            });
        }
    }

    let mut regularity = matches as f64 / binary.len() as f64;
    if remainder > 0 {
        regularity *= 1.0 - (0.05 * remainder as f64 / foot_len as f64);
    }

    let foot_adj = match name {
        "iamb" => "iambic",
        "trochee" => "trochaic",
        "anapest" => "anapestic",
        "dactyl" => "dactylic",
        "spondee" => "spondaic",
        "pyrrhic" => "pyrrhic",
        _ => name,
    };

    let count_name = if num_feet < FOOT_COUNTS.len() {
        FOOT_COUNTS[num_feet].to_string()
    } else {
        format!("{}-meter", num_feet)
    };

    MeterResult {
        foot_type: name.into(),
        foot_count: num_feet,
        meter_name: format!("{} {}", foot_adj, count_name),
        regularity: (regularity * 10000.0).round() / 10000.0,
        expected_pattern: expected,
        deviations,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Deviation {
    pub position: usize,
    pub expected: String,
    pub actual: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterResult {
    pub foot_type: String,
    pub foot_count: usize,
    pub meter_name: String,
    pub regularity: f64,
    pub expected_pattern: Vec<i32>,
    pub deviations: Vec<Deviation>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterComparison {
    pub same_meter: bool,
    pub same_length: bool,
    pub stress_agreement: f64,
    pub mismatch_positions: Vec<usize>,
}
